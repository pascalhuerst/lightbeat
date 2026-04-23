use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::ui::xy_pad::{XyPadDisplay, XyPadMode};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct XyPadWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    mode: XyPadMode,
    /// Local authoritative position; mirrored from engine display so both
    /// the knob drag (UI → engine) and param edits (inspector → engine)
    /// stay in sync.
    x: f32,
    y: f32,
}

impl XyPadWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            name: String::new(),
            mode: XyPadMode::Positions,
            x: 0.5, y: 0.5,
        }
    }

    fn sync_from_display(&mut self) {
        let s = self.shared.lock().unwrap();
        if let Some(d) = s.display.as_ref().and_then(|d| d.downcast_ref::<XyPadDisplay>()) {
            self.name = d.name.clone();
            self.mode = d.mode;
            self.x = d.x;
            self.y = d.y;
        }
    }

    fn push_position(&self) {
        // Engine params: 0 = Mode, 1 = X, 2 = Y.
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((1, ParamValue::Float(self.x)));
        s.pending_params.push((2, ParamValue::Float(self.y)));
    }

    fn push_name(&self) {
        // Name isn't expressible as a ParamValue, so push via pending_config;
        // engine's load_data merges the `name` field.
        let mut s = self.shared.lock().unwrap();
        s.pending_config = Some(serde_json::json!({ "name": self.name }));
    }
}

impl NodeWidget for XyPadWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "XY Pad" }
    fn title(&self) -> &str {
        if self.name.is_empty() { "XY Pad" } else { self.name.as_str() }
    }
    fn description(&self) -> &'static str {
        "Draggable point inside a unit square. Emits four corner weights \
         (bilinear) — use as mix weights between four sources or wire into \
         Palette to Gradient's pos1..pos4 for a continuous 4-way blend."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("q1", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("q2", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("q3", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("q4", PortType::Untyped)),
        ]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 120.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();

        let avail = ui.available_size();
        // Keep the pad square within the given rect so the circle's x/y
        // motion maps 1:1 on both axes regardless of resize direction.
        let side = avail.x.min(avail.y).max(40.0);
        let (resp, painter) = ui.allocate_painter(
            Vec2::splat(side),
            Sense::click_and_drag(),
        );
        let rect = resp.rect;

        // Pad body.
        painter.rect_filled(rect, 4.0, Color32::from_gray(28));

        // Crosshairs dividing the pad into four quadrants.
        let mid = rect.center();
        let grid_stroke = Stroke::new(1.0, Color32::from_gray(70));
        painter.line_segment([Pos2::new(rect.min.x, mid.y), Pos2::new(rect.max.x, mid.y)], grid_stroke);
        painter.line_segment([Pos2::new(mid.x, rect.min.y), Pos2::new(mid.x, rect.max.y)], grid_stroke);

        // Quadrant labels (q1 top-left, q2 top-right, q3 bottom-left, q4 bottom-right).
        let label_col = Color32::from_gray(110);
        let label_inset = 6.0;
        let font = egui::FontId::proportional(10.0);
        painter.text(Pos2::new(rect.min.x + label_inset, rect.min.y + label_inset),
            egui::Align2::LEFT_TOP, "q1", font.clone(), label_col);
        painter.text(Pos2::new(rect.max.x - label_inset, rect.min.y + label_inset),
            egui::Align2::RIGHT_TOP, "q2", font.clone(), label_col);
        painter.text(Pos2::new(rect.min.x + label_inset, rect.max.y - label_inset),
            egui::Align2::LEFT_BOTTOM, "q3", font.clone(), label_col);
        painter.text(Pos2::new(rect.max.x - label_inset, rect.max.y - label_inset),
            egui::Align2::RIGHT_BOTTOM, "q4", font, label_col);

        // Border.
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Inside);

        // Circle position in screen coords.
        let knob = Pos2::new(
            rect.min.x + self.x.clamp(0.0, 1.0) * rect.width(),
            rect.min.y + self.y.clamp(0.0, 1.0) * rect.height(),
        );

        // Guide lines from the knob to the axes so the current (x, y) reads
        // at a glance.
        let guide = Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 200, 240, 80));
        painter.line_segment([Pos2::new(rect.min.x, knob.y), Pos2::new(rect.max.x, knob.y)], guide);
        painter.line_segment([Pos2::new(knob.x, rect.min.y), Pos2::new(knob.x, rect.max.y)], guide);

        // Knob: filled cyan circle with a dark outline for contrast.
        let knob_r = 6.0;
        painter.circle_filled(knob, knob_r, theme::STATUS_ACTIVE);
        painter.circle_stroke(knob, knob_r, Stroke::new(1.5, Color32::from_gray(20)));

        // Interaction — click or drag anywhere inside the pad to reposition.
        let moved = resp.dragged() || resp.clicked() || resp.drag_started();
        if moved
            && let Some(p) = resp.interact_pointer_pos() {
                self.x = ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                self.y = ((p.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
                self.push_position();
            }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                self.push_name();
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
