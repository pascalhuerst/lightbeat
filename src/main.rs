mod beat_clock;
mod link_controller;
mod widgets;

use eframe::egui;
use widgets::LinkStatusNode;
use widgets::nodes::{NodeGraph, NodeId};

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};

struct LightBeatApp {
    graph: NodeGraph,
    _beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut graph = NodeGraph::new();

        // Create the Link status node.
        let link_node = LinkStatusNode::new(NodeId(1));
        let link_state = link_node.state.clone();
        graph.add_node(Box::new(link_node), egui::pos2(50.0, 50.0));

        // Start beat clock and subscribe the link node state.
        let beat_clock = BeatClock::new(4.0);
        let subs = vec![
            beat_clock.subscribe(BeatPattern::every(1), link_state),
        ];

        Self {
            graph,
            _beat_clock: beat_clock,
            _subs: subs,
        }
    }
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui);
        });
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
