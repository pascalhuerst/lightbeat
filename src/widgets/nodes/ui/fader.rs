use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::ui::fader::{FaderDisplay, FaderOrientation};
use crate::engine::types::*;
use crate::widgets::fader::{self, FaderStyle};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct FaderWidget {
    id: NodeId,
    shared: SharedState,
    orientation: FaderOrientation,
    value: f32,
}

impl FaderWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            orientation: FaderOrientation::Vertical,
            value: 0.0,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "orientation": self.orientation.as_str(),
            "value": self.value,
        }));
    }
}

impl NodeWidget for FaderWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Fader" }
    fn title(&self) -> &str { "Fader" }
    fn description(&self) -> &'static str {
        "Draggable fader outputting a value 0..1. Double-click to reset, shift-drag for fine-grained."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("out", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine display.
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<FaderDisplay>()) {
            self.orientation = d.orientation;
            self.value = d.value;
        }
        drop(shared);

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }

        let orient = match self.orientation {
            FaderOrientation::Vertical => fader::Orientation::Vertical,
            FaderOrientation::Horizontal => fader::Orientation::Horizontal,
        };
        let mut v = self.value;
        let resp = fader::fader(ui, avail, &mut v, orient, &FaderStyle::default(), false);
        if (v - self.value).abs() > f32::EPSILON || resp.double_clicked() {
            self.value = v;
            self.push_config();
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Orientation:");
            let mut changed = false;
            if ui.radio_value(&mut self.orientation, FaderOrientation::Vertical, "Vertical").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.orientation, FaderOrientation::Horizontal, "Horizontal").clicked() {
                changed = true;
            }
            if changed { self.push_config(); }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
