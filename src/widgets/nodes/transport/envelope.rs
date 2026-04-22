use std::any::Any;

use egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::engine::nodes::transport::envelope::EnvelopeDisplay;
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HANDLE_RADIUS: f32 = 4.0;
const CURVE_COLOR: Color32 = Color32::from_rgb(80, 200, 160);
const ACTIVE_COLOR: Color32 = Color32::from_rgb(120, 255, 200);
const HANDLE_COLOR: Color32 = Color32::from_rgb(220, 220, 220);
const BG_COLOR: Color32 = Color32::from_gray(30);

pub struct EnvelopeWidget {
    id: NodeId,
    shared: SharedState,
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl EnvelopeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            inputs: vec![
                PortDef::new("gate", PortType::Logic),
                PortDef::new("signal", PortType::Untyped),
            ],
            outputs: vec![
                PortDef::new("envelope", PortType::Untyped),
                PortDef::new("signal", PortType::Untyped),
            ],
        }
    }
}

impl NodeWidget for EnvelopeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "ADSR" }
    fn title(&self) -> &str { "ADSR" }
    fn description(&self) -> &'static str { "Attack-decay-sustain-release envelope shaping a value over time when triggered by a gate." }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        self.inputs.iter().map(UiPortDef::from_def).collect()
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(UiPortDef::from_def).collect()
    }

    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<EnvelopeDisplay>());

        let (stage, env_val, attack, decay, sustain, release) = if let Some(d) = display {
            (d.stage, d.envelope_value, d.attack, d.decay, d.sustain, d.release)
        } else {
            (0, 0.0, 0.01, 0.1, 0.7, 0.3)
        };
        drop(shared);

        let w = ui.available_width();
        let h = ui.available_height().max(60.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::click_and_drag());
        let rect = response.rect;

        painter.rect_filled(rect, 2.0, BG_COLOR);

        // Layout: divide the width into A, D, S(fixed), R sections.
        // Normalize times to fit the width. Max total time for display.
        let max_time = 5.0_f32;
        let total = (attack + decay + release).min(max_time);
        let sustain_width_frac = 0.2; // sustain gets 20% of width
        let env_width_frac = 1.0 - sustain_width_frac;

        let scale = if total > 0.0 { env_width_frac / total } else { env_width_frac };
        let a_w = attack.min(max_time) * scale * w;
        let d_w = decay.min(max_time) * scale * w;
        let s_w = sustain_width_frac * w;
        let r_w = release.min(max_time) * scale * w;

        // Key points of the ADSR curve.
        let x0 = rect.min.x;                    // start
        let x1 = x0 + a_w;                      // end of attack (peak)
        let x2 = x1 + d_w;                      // end of decay (sustain level)
        let x3 = x2 + s_w;                      // end of sustain
        let x4 = (x3 + r_w).min(rect.max.x);    // end of release

        let y_bot = rect.max.y - 2.0;           // 0.0 level
        let y_top = rect.min.y + 2.0;           // 1.0 level
        let y_sus = y_bot - sustain * (y_bot - y_top); // sustain level

        let p0 = Pos2::new(x0, y_bot);
        let p1 = Pos2::new(x1, y_top);           // attack peak
        let p2 = Pos2::new(x2, y_sus);           // decay -> sustain
        let p3 = Pos2::new(x3, y_sus);           // sustain hold
        let p4 = Pos2::new(x4, y_bot);           // release end

        // Draw filled area under curve.
        let fill_color = CURVE_COLOR.linear_multiply(0.15);
        let points = vec![p0, p1, p2, p3, p4, Pos2::new(x4, y_bot), Pos2::new(x0, y_bot)];
        painter.add(egui::Shape::convex_polygon(points, fill_color, Stroke::NONE));

        // Draw the curve lines.
        let stroke = Stroke::new(2.0, CURVE_COLOR);
        painter.line_segment([p0, p1], stroke);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], Stroke::new(2.0, CURVE_COLOR.linear_multiply(0.6)));
        painter.line_segment([p3, p4], stroke);

        // Draw current envelope position indicator.
        if stage > 0 {
            let indicator_y = y_bot - env_val * (y_bot - y_top);
            let indicator_x = match stage {
                1 => x0 + (env_val * a_w),                          // attack
                2 => x1 + ((1.0 - (env_val - sustain) / (1.0 - sustain).max(0.01)) * d_w), // decay
                3 => x2 + s_w * 0.5,                                 // sustain (middle)
                4 => x3 + ((1.0 - env_val / sustain.max(0.01)) * r_w), // release
                _ => x0,
            };
            painter.circle_filled(
                Pos2::new(indicator_x.clamp(rect.min.x, rect.max.x), indicator_y),
                HANDLE_RADIUS * zoom,
                ACTIVE_COLOR,
            );
        }

        // Draw draggable handles at the key points.
        let hr = HANDLE_RADIUS * zoom;
        painter.circle_filled(p1, hr, HANDLE_COLOR); // attack time
        painter.circle_filled(p2, hr, HANDLE_COLOR); // decay time + sustain level
        painter.circle_filled(p4, hr, HANDLE_COLOR); // release time

        // Handle dragging to adjust ADSR params.
        if (response.dragged() || response.clicked())
            && let Some(pos) = response.interact_pointer_pos() {
                let hit_radius = hr + 6.0;

                // Find closest handle.
                let handles = [(p1, 0u8), (p2, 1u8), (p4, 2u8)];
                if let Some((_, handle_id)) = handles.iter()
                    .filter(|(p, _)| p.distance(pos) < hit_radius)
                    .min_by(|(a, _), (b, _)| a.distance(pos).partial_cmp(&b.distance(pos)).unwrap())
                {
                    let mut shared = self.shared.lock().unwrap();
                    match handle_id {
                        0 => {
                            // Attack: horizontal = time
                            let new_attack = ((pos.x - x0) / w * (total / env_width_frac)).clamp(0.001, 5.0);
                            shared.pending_params.push((0, ParamValue::Float(new_attack)));
                        }
                        1 => {
                            // Decay: horizontal = time, vertical = sustain level
                            let new_decay = ((pos.x - x1) / w * (total / env_width_frac)).clamp(0.001, 5.0);
                            let new_sustain = 1.0 - ((pos.y - y_top) / (y_bot - y_top)).clamp(0.0, 1.0);
                            shared.pending_params.push((1, ParamValue::Float(new_decay)));
                            shared.pending_params.push((2, ParamValue::Float(new_sustain)));
                        }
                        2 => {
                            // Release: horizontal = time
                            let new_release = ((pos.x - x3) / w * (total / env_width_frac)).clamp(0.001, 10.0);
                            shared.pending_params.push((3, ParamValue::Float(new_release)));
                        }
                        _ => {}
                    }
                }
            }

        // Stage label (small, bottom-left).
        let stage_name = match stage {
            1 => "A", 2 => "D", 3 => "S", 4 => "R",
            _ => "",
        };
        if !stage_name.is_empty() {
            painter.text(
                Pos2::new(rect.min.x + 4.0, rect.max.y - 2.0),
                egui::Align2::LEFT_BOTTOM,
                stage_name,
                egui::FontId::monospace(9.0 * zoom),
                ACTIVE_COLOR,
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
