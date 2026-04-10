mod beat_clock;
mod link_controller;
mod widgets;

use eframe::egui;
use widgets::ExampleWidget;

struct LightBeatApp {
    // -- Active widget --
    // Comment/uncomment to switch which widget is displayed.
    example: ExampleWidget,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            example: ExampleWidget::new(),
        }
    }
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // -- Switch active widget by commenting/uncommenting --
            self.example.show(ui);
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
