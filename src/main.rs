mod beat_clock;
mod link_controller;
mod widgets;

use eframe::egui;
use widgets::{LinkStatus, StepSequencer};

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};

struct LightBeatApp {
    sequencer: StepSequencer,
    link_status: LinkStatus,
    _beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let sequencer = StepSequencer::new();
        let link_status = LinkStatus::new();
        let beat_clock = BeatClock::new(4.0);

        let subs = vec![
            beat_clock.subscribe(BeatPattern::every(1), sequencer.state.clone()),
            beat_clock.subscribe(BeatPattern::every(1), link_status.state.clone()),
        ];

        Self {
            sequencer,
            link_status,
            _beat_clock: beat_clock,
            _subs: subs,
        }
    }
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.link_status.show(ui);
            });
            ui.add_space(8.0);
            self.sequencer.show(ui);
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
