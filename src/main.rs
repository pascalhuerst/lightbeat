mod beat_clock;
mod link_controller;
mod widgets;

use std::sync::Arc;

use eframe::egui;
use widgets::nodes::{NodeGraph, NodeId};
use widgets::{ClockNode, PhaseScalerNode, ScopeNode, StepSequencerNode};

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};

struct LightBeatApp {
    graph: NodeGraph,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();

        let mut graph = NodeGraph::new();

        // Register node types for the context menu.
        let snap_for_factory = Arc::clone(&snapshot);
        graph.register_node("Clock", move |id| {
            Box::new(ClockNode::new(id, Arc::clone(&snap_for_factory)))
        });
        graph.register_node("Step Sequencer", |id| {
            Box::new(StepSequencerNode::new(id))
        });
        graph.register_node("Phase Scaler", |id| {
            Box::new(PhaseScalerNode::new(id))
        });
        graph.register_node("Scope", |id| Box::new(ScopeNode::new(id)));

        // Create an initial clock node.
        let clock = ClockNode::new(NodeId(1), Arc::clone(&snapshot));
        let clock_state = clock.state.clone();
        graph.add_node(Box::new(clock), egui::pos2(50.0, 50.0));

        let subs = vec![beat_clock.subscribe(BeatPattern::every(1), clock_state)];

        Self {
            graph,
            beat_clock,
            _subs: subs,
        }
    }

    fn wire_new_nodes(&mut self) {
        for new_node in self.graph.drain_new_nodes() {
            let node = self.graph.node_mut(new_node.index);

            if let Some(clock) = node.as_any_mut().downcast_mut::<ClockNode>() {
                let sub = self
                    .beat_clock
                    .subscribe(BeatPattern::every(1), clock.state.clone());
                self._subs.push(sub);
            }
        }
    }
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // Inspector panel on the right.
        egui::SidePanel::right("inspector")
            .default_width(250.0)
            .show(ctx, |ui| {
                if let Some(node) = self.graph.selected_node_mut() {
                    widgets::inspector::show_inspector(ui, node.as_mut());
                } else {
                    ui.heading("Inspector");
                    ui.separator();
                    ui.label("Select a node to inspect.");
                }
            });

        // Node graph fills the rest.
        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui);
        });

        self.wire_new_nodes();
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
