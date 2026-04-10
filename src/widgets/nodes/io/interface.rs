use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::interface::InterfaceDisplay;
use crate::engine::types::*;
use crate::objects::output::OutputConfig;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct InterfaceWidget {
    id: NodeId,
    shared: SharedState,
    pub editor_open: bool,
}

impl InterfaceWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            editor_open: false,
        }
    }

    pub fn show_editor(&mut self, ctx: &egui::Context) {
        if !self.editor_open {
            return;
        }

        let shared = self.shared.lock().unwrap();
        let config = shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<InterfaceDisplay>())
            .map(|d| d.config.clone());
        drop(shared);

        let Some(config) = config else { return };

        let mut open = self.editor_open;
        egui::Window::new("Interface Config")
            .id(egui::Id::new(("interface_editor", self.id.0)))
            .open(&mut open)
            .default_size([350.0, 200.0])
            .show(ctx, |ui| {
                match &config {
                    OutputConfig::ArtNet { host, port } => {
                        ui.heading("Art-Net Output");
                        ui.label(format!("Host: {}", host));
                        ui.label(format!("Port: {}", port));
                    }
                    OutputConfig::Sacn { source_name } => {
                        ui.heading("sACN Output");
                        ui.label(format!("Source: {}", source_name));
                    }
                    OutputConfig::None => {
                        ui.heading("No Output (Preview Only)");
                    }
                }

                ui.separator();
                ui.label("Configuration is edited via the Inspector panel.");
            });
        self.editor_open = open;
    }
}

fn config_label(config: &OutputConfig) -> String {
    match config {
        OutputConfig::ArtNet { host, .. } => format!("ArtNet ({})", host),
        OutputConfig::Sacn { source_name } => format!("sACN ({})", source_name),
        OutputConfig::None => "Preview".to_string(),
    }
}

impl NodeWidget for InterfaceWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Interface" }
    fn title(&self) -> &str { "Interface" }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 30.0 }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<InterfaceDisplay>());

        if let Some(d) = display {
            ui.colored_label(Color32::from_gray(180), config_label(&d.config));
        } else {
            ui.label("Not configured");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        if ui.button("Open Editor").clicked() {
            self.editor_open = true;
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
