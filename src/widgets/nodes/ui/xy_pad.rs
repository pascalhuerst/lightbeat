use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::ui::xy_pad::XyPadDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct XyPadWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
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
            x: 0.5, y: 0.5,
        }
    }

    fn sync_from_display(&mut self) {
        let s = self.shared.lock().unwrap();
        if let Some(d) = s.display.as_ref().and_then(|d| d.downcast_ref::<XyPadDisplay>()) {
            self.name = d.name.clone();
            self.x = d.x;
            self.y = d.y;
        }
    }

    fn push_position(&self) {
        // Engine params: 0 = X, 1 = Y.
        let mut s = self.shared.lock().unwrap();
        s.pending_params.push((0, ParamValue::Float(self.x)));
        s.pending_params.push((1, ParamValue::Float(self.y)));
    }

    fn push_name(&self) {
        let mut s = self.shared.lock().unwrap();
        s.pending_config = Some(serde_json::json!({ "name": self.name }));
    }
}

impl NodeWidget for XyPadWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "XY Pad" }
    fn title(&self) -> &str { &self.name }
    fn description(&self) -> &'static str {
        "Draggable point inside a unit square. Outputs the point's x and y \
         coordinates, each clamped to 0..1."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("x", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("y", PortType::Untyped)),
        ]
    }

    fn min_width(&self) -> f32 { 120.0 }
    fn min_content_height(&self) -> f32 { 120.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();

        let avail = ui.available_size();
        // Keep the pad square so x and y motion map 1:1 on both axes
        // regardless of resize direction.
        let side = avail.x.min(avail.y).max(40.0);
        let (resp, painter) = ui.allocate_painter(
            Vec2::splat(side),
            Sense::click_and_drag(),
        );
        let rect = resp.rect;

        painter.rect_filled(rect, 4.0, Color32::from_gray(28));

        // Crosshairs at the midpoint for visual reference.
        let mid = rect.center();
        let grid_stroke = Stroke::new(1.0, Color32::from_gray(70));
        painter.line_segment([Pos2::new(rect.min.x, mid.y), Pos2::new(rect.max.x, mid.y)], grid_stroke);
        painter.line_segment([Pos2::new(mid.x, rect.min.y), Pos2::new(mid.x, rect.max.y)], grid_stroke);

        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Inside);

        let knob = Pos2::new(
            rect.min.x + self.x.clamp(0.0, 1.0) * rect.width(),
            rect.min.y + self.y.clamp(0.0, 1.0) * rect.height(),
        );

        // Guide lines from the knob to the axes so the current (x, y) reads
        // at a glance.
        let guide = Stroke::new(1.0, Color32::from_rgba_unmultiplied(80, 200, 240, 80));
        painter.line_segment([Pos2::new(rect.min.x, knob.y), Pos2::new(rect.max.x, knob.y)], guide);
        painter.line_segment([Pos2::new(knob.x, rect.min.y), Pos2::new(knob.x, rect.max.y)], guide);

        let knob_r = 6.0;
        painter.circle_filled(knob, knob_r, theme::SEM_PRIMARY);
        painter.circle_stroke(knob, knob_r, Stroke::new(1.5, Color32::from_gray(20)));

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
