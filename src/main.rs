mod beat_clock;
mod link_controller;
mod widgets;

use eframe::egui;
use widgets::nodes::{NodeGraph, NodeId};
use widgets::{ClockNode, StepSequencerNode};

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};

struct LightBeatApp {
    graph: NodeGraph,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut graph = NodeGraph::new();

        // Register node types for the context menu.
        graph.register_node("Clock", |id| Box::new(ClockNode::new(id)));
        graph.register_node("Step Sequencer", |id| Box::new(StepSequencerNode::new(id)));

        // Create an initial clock node.
        let clock = ClockNode::new(NodeId(1));
        let clock_state = clock.state.clone();
        graph.add_node(Box::new(clock), egui::pos2(50.0, 50.0));

        let beat_clock = BeatClock::new(4.0);
        let subs = vec![beat_clock.subscribe(BeatPattern::every(1), clock_state)];

        Self {
            graph,
            beat_clock,
            _subs: subs,
        }
    }

    /// Wire up any nodes that were just spawned from the context menu.
    fn wire_new_nodes(&mut self) {
        for new_node in self.graph.drain_new_nodes() {
            let node = self.graph.node_mut(new_node.index);

            // Only Clock nodes need a direct beat clock subscription.
            // Other nodes receive signals through graph connections.
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

        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui);
        });

        self.wire_new_nodes();
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
