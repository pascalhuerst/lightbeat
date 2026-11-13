use egui::{Color32, Ui};

use crate::engine::types::*;
use crate::widgets::fader;
use crate::widgets::nodes::node::{NodeWidget, node_identity};
use crate::widgets::nodes::types::UiPortDef;
use crate::widgets::stop_handles::{self, COUNT};

/// The positions of an evenly spread gradient, which is what a node starts with.
const EVEN: [f32; COUNT] = [0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0];

fn pos_name(i: usize) -> String {
    format!("Pos {}", i + 1)
}

pub struct PaletteToGradientWidget {
    id: NodeId,
    shared: SharedState,
    /// The handle being dragged, if any.
    drag: Option<usize>,
    /// What the user just set, until the engine's copy (which reaches the UI
    /// every few ticks) has caught up, so a handle does not jump back.
    sent: [Option<f32>; COUNT],
}

impl PaletteToGradientWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, drag: None, sent: [None; COUNT] }
    }

    fn set_position(&mut self, i: usize, v: f32) {
        self.sent[i] = Some(v);
        self.shared.lock().pending_params.push((i, ParamValue::Float(v)));
    }

    /// The positions as the engine holds them, the colours of the palette
    /// wired in, and which positions a wire is driving.
    fn read(&mut self) -> ([f32; COUNT], [Color32; COUNT], [bool; COUNT]) {
        // Before locking: ui_inputs may lock the shared state itself.
        let native = self.ui_inputs().len();
        let s = self.shared.lock();
        let mut positions = EVEN;
        for (i, pos) in positions.iter_mut().enumerate() {
            if let Some(ParamDef::Float { value, .. }) = s.current_params.iter().find(|p| p.name() == pos_name(i)) {
                *pos = *value;
            }
        }
        let channel = |c: usize| (s.inputs.get(c).copied().unwrap_or(0.0).clamp(0.0, 1.0) * 255.0) as u8;
        let colors = std::array::from_fn(|i| Color32::from_rgb(channel(i * 3), channel(i * 3 + 1), channel(i * 3 + 2)));
        let locked: [bool; COUNT] = std::array::from_fn(|i| {
            let name = pos_name(i);
            s.promoted
                .iter()
                .position(|p| p.name == name)
                .is_some_and(|slot| s.inputs_connected.get(native + slot).copied().unwrap_or(false))
        });
        // A wire overwrites the value every tick, so it never "catches up".
        for ((pos, sent), &is_locked) in positions.iter_mut().zip(&mut self.sent).zip(&locked) {
            match *sent {
                Some(v) if !is_locked && (v - *pos).abs() > 1e-3 => *pos = v,
                _ => *sent = None,
            }
        }
        (positions, colors, locked)
    }
}

impl NodeWidget for PaletteToGradientWidget {
    node_identity!("Palette to Gradient");
    fn description(&self) -> &'static str {
        "Builds a 4-stop gradient from a palette and one position per palette colour. \
         Drag the handles to move each stop (Shift for fine steps, double-click to reset). \
         A new node has the four positions as inputs (`Pos 1..Pos 4`); a handle whose input \
         is wired shows the incoming value and cannot be dragged."
    }

    fn default_promoted(&self) -> &'static [(&'static str, PromotedKind)] {
        &[
            ("Pos 1", PromotedKind::Float),
            ("Pos 2", PromotedKind::Float),
            ("Pos 3", PromotedKind::Float),
            ("Pos 4", PromotedKind::Float),
        ]
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().0) }
    fn ui_outputs(&self) -> Vec<UiPortDef> { UiPortDef::from_defs(&crate::engine::nodes::math::palette_to_gradient::ports().1) }

    fn min_width(&self) -> f32 { 150.0 }
    fn min_content_height(&self) -> f32 { stop_handles::MIN_HEIGHT }
    fn resizable(&self) -> bool { true }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let (positions, colors, locked) = self.read();
        let response = stop_handles::show(ui, &mut self.drag, &positions, &colors, &locked);
        if let Some((i, v)) = response.moved {
            self.set_position(i, v);
        }
        if let Some(i) = response.reset {
            let default = fader::recorded_default(&self.shared, i).unwrap_or(EVEN[i]);
            self.set_position(i, default);
        }
    }
}
