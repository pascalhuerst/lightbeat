use std::any::Any;
use egui::{self, Color32, Ui, Vec2, Sense};

use crate::engine::nodes::math::color_components::{ColorMode, ColorOpsDisplay};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

fn show_color_swatch(ui: &mut Ui, rgb: [f32; 3]) {
    let color = Color32::from_rgb(
        (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
        (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
    );
    let (resp, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width(), 14.0),
        Sense::hover(),
    );
    painter.rect_filled(resp.rect, 2.0, color);
}

/// The ports on the component side (inputs for Components to Color, outputs
/// for Color to Components).
fn component_ports(mode: ColorMode) -> Vec<PortDef> {
    match mode {
        ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
        _ => mode.channel_names().iter().map(|n| PortDef::new(*n, PortType::Untyped)).collect(),
    }
}

/// The single port on the colour side.
fn color_port(mode: ColorMode) -> Vec<PortDef> {
    match mode {
        ColorMode::Neutral => vec![PortDef::new("?", PortType::Any)],
        _ => vec![PortDef::new("color", PortType::Color)],
    }
}

/// Push a mode change to the engine via the standard param channel.
fn push_mode(shared: &SharedState, mode: ColorMode) {
    let mut s = shared.lock().unwrap();
    s.pending_params.push((0, ParamValue::Choice(mode.to_index())));
}

/// A wire on either side of a still-undecided node makes it an RGB node. The
/// inspector can switch to HSV, RGBW or CMY afterwards.
fn infer_mode(current: ColorMode, colour_side_wire: Option<PortType>) -> ColorMode {
    if current != ColorMode::Neutral {
        return current;
    }
    match colour_side_wire {
        // The colour side only means something if a Color is on it.
        Some(t) if t != PortType::Color => ColorMode::Neutral,
        _ => ColorMode::Rgb,
    }
}

fn show_node_content(ui: &mut Ui, shared: &SharedState, own_mode: &mut ColorMode) {
    let s = shared.lock().unwrap();
    let display = s.display.as_ref().and_then(|d| d.downcast_ref::<ColorOpsDisplay>());
    let (mode, rgb) = match display {
        Some(d) => (d.mode, d.rgb),
        None => (*own_mode, [0.0; 3]),
    };
    drop(s);
    *own_mode = mode;
    match mode {
        ColorMode::Neutral => {
            ui.colored_label(theme::TEXT_DIM, "Connect to set type");
        }
        _ => show_color_swatch(ui, rgb),
    }
}

// ---------------------------------------------------------------------------
// Components to Color widget
// ---------------------------------------------------------------------------

pub struct ComponentsToColorWidget {
    id: NodeId,
    shared: SharedState,
    mode: ColorMode,
}

impl ComponentsToColorWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: ColorMode::Neutral }
    }
}

impl NodeWidget for ComponentsToColorWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Components to Color" }
    fn description(&self) -> &'static str {
        "Combines components (RGB, HSV, RGBW or CMY) into a color. The mode is picked from the first connection."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        component_ports(self.mode).iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        color_port(self.mode).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, _source_type: PortType) {
        let new_mode = infer_mode(self.mode, None);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, dest_type: PortType) {
        let new_mode = infer_mode(self.mode, Some(dest_type));
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        show_node_content(ui, &self.shared, &mut self.mode);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Color to Components widget
// ---------------------------------------------------------------------------

pub struct ColorToComponentsWidget {
    id: NodeId,
    shared: SharedState,
    mode: ColorMode,
}

impl ColorToComponentsWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, mode: ColorMode::Neutral }
    }
}

impl NodeWidget for ColorToComponentsWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color to Components" }
    fn description(&self) -> &'static str {
        "Splits a color into components (RGB, HSV, RGBW or CMY). The mode is picked from the first connection."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        color_port(self.mode).iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        component_ports(self.mode).iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 100.0 }
    fn min_content_height(&self) -> f32 { 25.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn on_ui_connect(&mut self, _input_port: usize, source_type: PortType) {
        let new_mode = infer_mode(self.mode, Some(source_type));
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }
    fn on_ui_output_connect(&mut self, _output_port: usize, _dest_type: PortType) {
        let new_mode = infer_mode(self.mode, None);
        if new_mode != self.mode {
            self.mode = new_mode;
            push_mode(&self.shared, new_mode);
        }
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        show_node_content(ui, &self.shared, &mut self.mode);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
