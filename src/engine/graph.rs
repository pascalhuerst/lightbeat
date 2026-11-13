use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ringbuf::traits::Consumer;

use super::CommandConsumer;
use super::nodes::meta::subgraph::{InnerGraph, SubgraphProcessNode};
use super::effects::{EffectHub, RenderGate};
use super::types::*;
use crate::dmx_io::{DmxOutputManager, SharedDmxState, SharedObjectStore};
use crate::perf::{EngineTickTimer, SharedPerfStats};

/// The engine-side signal graph. Runs on its own thread at ~1kHz. The graph
/// itself is an `InnerGraph`, the same core every subgraph runs, with empty
/// bridges; what lives here is only what the root alone does: the hover
/// probe, effect flattening and the DMX output.
pub struct EngineGraph {
    core: InnerGraph,
    dmx: Option<DmxOutputManager>,
    /// Where effect layers meet; flattened into the objects once per frame.
    effect_hub: EffectHub,
    flatten_gate: RenderGate,
    object_store: SharedObjectStore,
    perf: SharedPerfStats,
}

/// How often the engine mirrors its state into shared state for the UI.
/// The UI redraws at ~60 fps, so publishing at 1 kHz threw away 15 of every
/// 16 updates while paying for `params()` allocation, `save_data()` JSON
/// construction and display buffer clones on every one of them.
pub(crate) const UI_SYNC_EVERY_N_TICKS: u32 = 16;

impl EngineGraph {
    pub fn new(
        dmx_shared: SharedDmxState,
        object_store: SharedObjectStore,
        effect_hub: EffectHub,
        perf: SharedPerfStats,
    ) -> Self {
        Self {
            core: InnerGraph::new_root(),
            effect_hub,
            flatten_gate: RenderGate::new(),
            object_store: object_store.clone(),
            dmx: Some(DmxOutputManager::new(dmx_shared, object_store)),
            perf,
        }
    }

    fn apply_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::AddNode { node, shared } => {
                self.core.apply(SubgraphInnerCmd::AddNode { node, shared });
            }
            EngineCommand::RemoveNode(id) => {
                self.core.apply(SubgraphInnerCmd::RemoveNode(id));
            }
            EngineCommand::AddConnection(conn) => {
                self.core.apply(SubgraphInnerCmd::AddConnection(conn));
            }
            EngineCommand::RemoveConnectionTo(to) => {
                self.core.apply(SubgraphInnerCmd::RemoveConnectionTo(to));
            }
            EngineCommand::NotifyConnect {
                node_id,
                input_port,
                source_type,
            } => {
                self.core.apply(SubgraphInnerCmd::NotifyConnect { node_id, input_port, source_type });
            }
            EngineCommand::NotifyDisconnect {
                node_id,
                input_port,
            } => {
                self.core.apply(SubgraphInnerCmd::NotifyDisconnect { node_id, input_port });
            }
            EngineCommand::LoadData { node_id, data } => {
                self.core.apply(SubgraphInnerCmd::LoadData { node_id, data });
            }
            EngineCommand::RemoveAllNodes => {
                self.core.clear();
            }
            EngineCommand::SetInterfaces(interfaces) => {
                if let Some(dmx) = &mut self.dmx {
                    dmx.set_interfaces(interfaces);
                }
            }
            EngineCommand::SubgraphInnerCommand { subgraph_path, command } => {
                self.core.mark_plan_dirty();
                self.apply_subgraph_inner_cmd(&subgraph_path, *command);
            }
        }
    }

    /// Defensive recursion guard — prevents accidental runaway descent if the
    /// path is malformed. The user is unlikely to nest this deep manually.
    const MAX_SUBGRAPH_NEST_DEPTH: usize = 32;

    fn apply_subgraph_inner_cmd(&mut self, path: &[NodeId], cmd: SubgraphInnerCmd) {
        if path.is_empty() { return; }
        if path.len() > Self::MAX_SUBGRAPH_NEST_DEPTH {
            eprintln!(
                "Subgraph nesting depth {} exceeds limit {}; dropping command",
                path.len(), Self::MAX_SUBGRAPH_NEST_DEPTH,
            );
            return;
        }

        // Find the outermost target subgraph.
        let Some(node) = self.core.nodes.iter_mut().find(|n| n.node_id() == path[0]) else {
            return;
        };
        let Some(sg) = node.as_any_mut().and_then(|a| a.downcast_mut::<SubgraphProcessNode>()) else {
            return;
        };
        sg.apply_inner_cmd_at_path(&path[1..], cmd);
    }

    /// One tick of the engine: the shared graph core (take pending, plan,
    /// process, propagate, UI mirror), then what only the root does.
    fn tick(&mut self) {
        self.core.tick_with(&[], &[], |core| {
            if let Some(t) = super::probe::target()
                && let Some(i) = core.index_of(t.node)
            {
                let v = if t.output { core.nodes[i].read_output(t.channel) } else { core.nodes[i].read_input(t.channel) };
                super::probe::record(v);
            }
        });

        let mut clock = crate::perf::PhaseClock::start();
        // Blend the effect stacks into the LEDs, then tick the DMX output
        // manager (merge overrides, send to interfaces).
        if self.flatten_gate.due() {
            self.effect_hub.flatten(&self.object_store);
        }
        if let Some(dmx) = &mut self.dmx {
            dmx.tick();
        }
        clock.lap("[root] 5 dmx");
    }

    /// Run the engine loop. Called from the engine thread. `stopped` is set
    /// when a tick panics and the loop gives up.
    /// `command_rx` is the consumer end of the ring buffer.
    pub fn run(mut self, mut command_rx: CommandConsumer, stopped: Arc<AtomicBool>) {
        let perf = self.perf.clone();
        let mut timer = EngineTickTimer::default();
        // Absolute deadlines: sleeping a fixed 1 ms after each tick would make
        // the period tick time plus sleep overshoot, and every node that
        // integrates time would run slow by that much.
        const PERIOD: Duration = Duration::from_millis(1);
        const RESYNC_AFTER: Duration = Duration::from_millis(5);
        let mut deadline = Instant::now() + PERIOD;
        let mut last_start = Instant::now();
        loop {
            // Drain all pending commands (non-blocking).
            while let Some(cmd) = command_rx.try_pop() {
                self.apply_command(cmd);
            }

            let started = Instant::now();
            super::time::set_dt(started.duration_since(last_start).as_secs_f32());
            last_start = started;
            // A panicking node must not leave the thread silently dead or
            // loop on the same broken graph: say so once and stop ticking.
            let ticked = catch_unwind(AssertUnwindSafe(|| timer.measure(&perf, || self.tick())));
            if let Err(payload) = ticked {
                let what = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                eprintln!("Engine stopped: a tick panicked: {what}");
                stopped.store(true, Ordering::SeqCst);
                return;
            }

            let now = Instant::now();
            if now > deadline + RESYNC_AFTER {
                // Far behind (stall, suspended laptop): do not burst to catch up.
                deadline = now + PERIOD;
            } else {
                deadline += PERIOD;
                if deadline > now {
                    std::thread::sleep(deadline - now);
                }
            }
        }
    }
}
