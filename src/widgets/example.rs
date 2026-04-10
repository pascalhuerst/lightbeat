use egui::Ui;

pub struct ExampleWidget {
    label: String,
    counter: u32,
}

impl ExampleWidget {
    pub fn new() -> Self {
        Self {
            label: "Hello, LightBeat!".to_string(),
            counter: 0,
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.heading("Example Widget");
        ui.separator();
        ui.label(&self.label);
        if ui.button("Click me").clicked() {
            self.counter += 1;
            self.label = format!("Clicked {} times", self.counter);
        }
    }
}
