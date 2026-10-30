use std::any::Any;

use egui::{self, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::transport::envelope::{EnvelopeDisplay, EnvelopeKind};
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const HANDLE_RADIUS: f32 = 5.0;
/// Max time values the engine accepts — used both to clamp drag output and
/// to cap horizontal display scale so extreme values don't collapse the
/// whole curve onto a sliver.
const A_MAX: f32 = 1.0;
const D_MAX: f32 = 1.0;
const R_MAX: f32 = 2.0;

pub struct EnvelopeWidget {
    id: NodeId,
    shared: SharedState,
    kind: EnvelopeKind,
}

impl EnvelopeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, kind: EnvelopeKind::Adsr }
    }

    /// Apply restored state from a saved param snapshot before
    /// `cleanup_stale_connections` runs. Called from project load, move-
    /// to-subgraph, and paste paths — mirrors the other mode-switching
    /// nodes.
    pub fn set_state_from_load(&mut self, kind: EnvelopeKind) {
        self.kind = kind;
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<EnvelopeDisplay>())
            && d.kind != self.kind {
                self.kind = d.kind;
            }
    }
}

impl NodeWidget for EnvelopeWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Envelope" }
    fn description(&self) -> &'static str {
        "Shapes a value over time. Mode picks the flavor: ADSR/AD/AR \
         trigger from a gate; Follower / Follower-RF extract an envelope \
         from an incoming signal; Slope \u{2197}/\u{2198} slew only the \
         rising (or only the falling) edge of the signal."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        // Port 0 always maps to the first input channel so wires from
        // previous modes survive a mode switch. The label flips between
        // "gate" (triggered modes) and "trigger" / "(unused)" elsewhere.
        let port0_name = if self.kind.uses_gate() { "gate" } else { "(unused)" };
        vec![
            UiPortDef::from_def(&PortDef::new(port0_name, PortType::Logic)),
            UiPortDef::from_def(&PortDef::new("signal", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("envelope", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("signal", PortType::Untyped)),
        ]
    }

    fn overridden_param_indices(&self) -> Vec<usize> {
        // Hide params that don't apply to the current mode. Indices:
        // 0=Attack/Rise/Tau, 1=Decay, 2=Sustain, 3=Release/Fall, 4=Mode.
        match self.kind {
            EnvelopeKind::Adsr => vec![],
            EnvelopeKind::Ad => vec![2, 3],
            EnvelopeKind::Ar => vec![1, 2],
            EnvelopeKind::Follower => vec![1, 2, 3],
            EnvelopeKind::FollowerRf => vec![1, 2],
            EnvelopeKind::SlopeUp => vec![1, 2, 3],
            EnvelopeKind::SlopeDown => vec![0, 1, 2],
        }
    }

    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 { 80.0 }
    fn resizable(&self) -> bool { true }

    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, zoom: f32) {
        self.sync_from_display();

        let shared = self.shared.lock().unwrap();
        let display = shared.display.as_ref()
            .and_then(|d| d.downcast_ref::<EnvelopeDisplay>());
        let (kind, stage, env_val, attack, decay, sustain, release) = if let Some(d) = display {
            (d.kind, d.stage, d.envelope_value, d.attack, d.decay, d.sustain, d.release)
        } else {
            (self.kind, 0, 0.0, 0.02, 0.08, 0.6, 0.15)
        };
        drop(shared);

        let w = ui.available_width();
        let h = ui.available_height().max(60.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(w, h), Sense::click_and_drag());
        let rect = response.rect;

        painter.rect_filled(rect, 3.0, theme::BG_GRID);
        let active = stage > 0 || matches!(
            kind,
            EnvelopeKind::Follower | EnvelopeKind::FollowerRf
                | EnvelopeKind::SlopeUp | EnvelopeKind::SlopeDown,
        ) && env_val.abs() > 0.001;
        let border_color = if active { theme::SEM_SUCCESS } else { theme::STROKE };
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, border_color), StrokeKind::Inside);

        match kind {
            EnvelopeKind::Adsr =>
                self.draw_adsr(&painter, rect, w, zoom, stage, env_val, attack, decay, sustain, release, Some(&response)),
            EnvelopeKind::Ad =>
                self.draw_ad(&painter, rect, w, zoom, stage, env_val, attack, decay, Some(&response)),
            EnvelopeKind::Ar =>
                self.draw_ar(&painter, rect, w, zoom, stage, env_val, attack, release, Some(&response)),
            EnvelopeKind::Follower
            | EnvelopeKind::FollowerRf
            | EnvelopeKind::SlopeUp
            | EnvelopeKind::SlopeDown =>
                self.draw_level(&painter, rect, env_val, kind),
        }

        // Mode label (small, top-right) so the user doesn't need to look
        // in the inspector to tell them apart.
        painter.text(
            Pos2::new(rect.max.x - 4.0, rect.min.y + 2.0),
            egui::Align2::RIGHT_TOP,
            kind.label(),
            egui::FontId::monospace(9.0 * zoom),
            theme::TEXT,
        );

        // Stage letter for gated modes (bottom-left).
        let stage_name = match stage {
            1 => "A", 2 => "D", 3 => if matches!(kind, EnvelopeKind::Ar) { "H" } else { "S" }, 4 => "R",
            _ => "",
        };
        if !stage_name.is_empty() {
            painter.text(
                Pos2::new(rect.min.x + 4.0, rect.max.y - 2.0),
                egui::Align2::LEFT_BOTTOM,
                stage_name,
                egui::FontId::monospace(9.0 * zoom),
                theme::TEXT_BRIGHT,
            );
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ---------------------------------------------------------------------------
// Per-mode drawing
// ---------------------------------------------------------------------------

impl EnvelopeWidget {
    #[allow(clippy::too_many_arguments)]
    fn draw_adsr(
        &self, painter: &egui::Painter, rect: egui::Rect, w: f32, zoom: f32,
        stage: u8, env_val: f32,
        attack: f32, decay: f32, sustain: f32, release: f32,
        response: Option<&egui::Response>,
    ) {
        let a_disp = attack.min(A_MAX);
        let d_disp = decay.min(D_MAX);
        let r_disp = release.min(R_MAX);
        let total = a_disp + d_disp + r_disp;
        let sustain_width_frac = 0.2;
        let env_width_frac = 1.0 - sustain_width_frac;
        let scale = if total > 0.0 { env_width_frac / total } else { env_width_frac };
        let a_w = a_disp * scale * w;
        let d_w = d_disp * scale * w;
        let s_w = sustain_width_frac * w;
        let r_w = r_disp * scale * w;

        let x0 = rect.min.x;
        let x1 = x0 + a_w;
        let x2 = x1 + d_w;
        let x3 = x2 + s_w;
        let x4 = (x3 + r_w).min(rect.max.x);
        let y_bot = rect.max.y - 2.0;
        let y_top = rect.min.y + 2.0;
        let y_sus = y_bot - sustain * (y_bot - y_top);

        let p0 = Pos2::new(x0, y_bot);
        let p1 = Pos2::new(x1, y_top);
        let p2 = Pos2::new(x2, y_sus);
        let p3 = Pos2::new(x3, y_sus);
        let p4 = Pos2::new(x4, y_bot);

        let fill = theme::SEM_SUCCESS.linear_multiply(0.15);
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2, p3, p4, Pos2::new(x4, y_bot), Pos2::new(x0, y_bot)],
            fill, Stroke::NONE));

        let stroke = Stroke::new(2.0_f32, theme::SEM_SUCCESS);
        painter.line_segment([p0, p1], stroke);
        painter.line_segment([p1, p2], stroke);
        painter.line_segment([p2, p3], Stroke::new(2.0_f32, theme::SEM_SUCCESS.linear_multiply(0.6)));
        painter.line_segment([p3, p4], stroke);

        if stage > 0 {
            let indicator_y = y_bot - env_val * (y_bot - y_top);
            let indicator_x = match stage {
                1 => x0 + (env_val * a_w),
                2 => x1 + ((1.0 - (env_val - sustain) / (1.0 - sustain).max(0.01)) * d_w),
                3 => x2 + s_w * 0.5,
                4 => x3 + ((1.0 - env_val / sustain.max(0.01)) * r_w),
                _ => x0,
            };
            painter.circle_filled(
                Pos2::new(indicator_x.clamp(rect.min.x, rect.max.x), indicator_y),
                HANDLE_RADIUS * zoom, theme::TEXT_BRIGHT);
        }

        self.draw_handles_and_drag(
            painter, rect, w, zoom, response,
            &[
                (p1, HandleId::Attack(a_w, env_width_frac, total)),
                (p2, HandleId::DecaySustain(x1, a_w, d_w, env_width_frac, total, y_top, y_bot)),
                (p4, HandleId::Release(x3, r_w, env_width_frac, total)),
            ],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_ad(
        &self, painter: &egui::Painter, rect: egui::Rect, w: f32, zoom: f32,
        stage: u8, env_val: f32, attack: f32, decay: f32,
        response: Option<&egui::Response>,
    ) {
        let a_disp = attack.min(A_MAX);
        let d_disp = decay.min(D_MAX);
        let total = (a_disp + d_disp).max(0.001);
        let scale = w / total;
        let a_w = a_disp * scale;
        let d_w = d_disp * scale;

        let x0 = rect.min.x;
        let x1 = x0 + a_w;
        let x2 = (x1 + d_w).min(rect.max.x);
        let y_bot = rect.max.y - 2.0;
        let y_top = rect.min.y + 2.0;

        let p0 = Pos2::new(x0, y_bot);
        let p1 = Pos2::new(x1, y_top);
        let p2 = Pos2::new(x2, y_bot);

        let fill = theme::SEM_SUCCESS.linear_multiply(0.15);
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2], fill, Stroke::NONE));
        let stroke = Stroke::new(2.0_f32, theme::SEM_SUCCESS);
        painter.line_segment([p0, p1], stroke);
        painter.line_segment([p1, p2], stroke);

        if stage > 0 {
            let indicator_y = y_bot - env_val * (y_bot - y_top);
            let indicator_x = match stage {
                1 => x0 + env_val * a_w,
                2 => x1 + (1.0 - env_val) * d_w,
                _ => x0,
            };
            painter.circle_filled(
                Pos2::new(indicator_x.clamp(rect.min.x, rect.max.x), indicator_y),
                HANDLE_RADIUS * zoom, theme::TEXT_BRIGHT);
        }

        self.draw_handles_and_drag(
            painter, rect, w, zoom, response,
            &[
                (p1, HandleId::AdAttack(a_w, total, w)),
                (p2, HandleId::AdDecay(x1, d_w, total, w)),
            ],
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_ar(
        &self, painter: &egui::Painter, rect: egui::Rect, w: f32, zoom: f32,
        stage: u8, env_val: f32, attack: f32, release: f32,
        response: Option<&egui::Response>,
    ) {
        let a_disp = attack.min(A_MAX);
        let r_disp = release.min(R_MAX);
        let hold_frac = 0.3;
        let env_width_frac = 1.0 - hold_frac;
        let total = (a_disp + r_disp).max(0.001);
        let scale = env_width_frac * w / total;
        let a_w = a_disp * scale;
        let h_w = hold_frac * w;
        let r_w = r_disp * scale;

        let x0 = rect.min.x;
        let x1 = x0 + a_w;
        let x2 = x1 + h_w;
        let x3 = (x2 + r_w).min(rect.max.x);
        let y_bot = rect.max.y - 2.0;
        let y_top = rect.min.y + 2.0;

        let p0 = Pos2::new(x0, y_bot);
        let p1 = Pos2::new(x1, y_top);
        let p2 = Pos2::new(x2, y_top);
        let p3 = Pos2::new(x3, y_bot);

        let fill = theme::SEM_SUCCESS.linear_multiply(0.15);
        painter.add(egui::Shape::convex_polygon(
            vec![p0, p1, p2, p3], fill, Stroke::NONE));
        let stroke = Stroke::new(2.0_f32, theme::SEM_SUCCESS);
        painter.line_segment([p0, p1], stroke);
        painter.line_segment([p1, p2], Stroke::new(2.0_f32, theme::SEM_SUCCESS.linear_multiply(0.6)));
        painter.line_segment([p2, p3], stroke);

        if stage > 0 {
            let indicator_y = y_bot - env_val * (y_bot - y_top);
            let indicator_x = match stage {
                1 => x0 + env_val * a_w,
                3 => x1 + h_w * 0.5,
                4 => x2 + (1.0 - env_val) * r_w,
                _ => x0,
            };
            painter.circle_filled(
                Pos2::new(indicator_x.clamp(rect.min.x, rect.max.x), indicator_y),
                HANDLE_RADIUS * zoom, theme::TEXT_BRIGHT);
        }

        self.draw_handles_and_drag(
            painter, rect, w, zoom, response,
            &[
                (p1, HandleId::ArAttack(a_w, env_width_frac, total, w)),
                (p3, HandleId::ArRelease(x2, r_w, env_width_frac, total, w)),
            ],
        );
    }

    fn draw_level(&self, painter: &egui::Painter, rect: egui::Rect, env_val: f32, kind: EnvelopeKind) {
        // Bipolar bar for slope modes (signal can go negative/positive),
        // unipolar for follower modes (signal is |input|).
        let bipolar = matches!(kind, EnvelopeKind::SlopeUp | EnvelopeKind::SlopeDown);
        let pad = 6.0;
        let bar_rect = egui::Rect::from_min_max(
            Pos2::new(rect.min.x + pad, rect.min.y + pad),
            Pos2::new(rect.max.x - pad, rect.max.y - pad),
        );

        if bipolar {
            // Zero line in the middle; bar extends up for positive, down
            // for negative values.
            let mid_y = bar_rect.center().y;
            painter.line_segment(
                [Pos2::new(bar_rect.min.x, mid_y), Pos2::new(bar_rect.max.x, mid_y)],
                Stroke::new(1.0_f32, theme::STROKE),
            );
            let half_h = (bar_rect.height() * 0.5) * env_val.clamp(-1.0, 1.0);
            let (y0, y1) = if half_h >= 0.0 { (mid_y - half_h, mid_y) } else { (mid_y, mid_y - half_h) };
            let fill_rect = egui::Rect::from_min_max(
                Pos2::new(bar_rect.min.x, y0), Pos2::new(bar_rect.max.x, y1));
            painter.rect_filled(fill_rect, 2.0, theme::SEM_SUCCESS);
        } else {
            // 0..1 bar bottom-up.
            let filled_h = bar_rect.height() * env_val.clamp(0.0, 1.0);
            let fill_rect = egui::Rect::from_min_max(
                Pos2::new(bar_rect.min.x, bar_rect.max.y - filled_h),
                Pos2::new(bar_rect.max.x, bar_rect.max.y),
            );
            painter.rect_filled(fill_rect, 2.0, theme::SEM_SUCCESS);
        }
    }

    fn draw_handles_and_drag(
        &self, painter: &egui::Painter, rect: egui::Rect, _w: f32, zoom: f32,
        response: Option<&egui::Response>,
        handles: &[(Pos2, HandleId)],
    ) {
        let hr = HANDLE_RADIUS * zoom;
        let hit_radius = hr + 10.0 * zoom;
        let ui_ctx = painter.ctx();
        let hover_pos = ui_ctx.input(|i| i.pointer.hover_pos());
        let hovered_idx = hover_pos.and_then(|hp| {
            handles.iter().enumerate()
                .filter(|(_, (p, _))| p.distance(hp) < hit_radius)
                .min_by(|(_, (a, _)), (_, (b, _))| a.distance(hover_pos.unwrap())
                    .partial_cmp(&b.distance(hover_pos.unwrap())).unwrap())
                .map(|(i, _)| i)
        });
        if hover_pos.map(|hp| rect.contains(hp)).unwrap_or(false) && hovered_idx.is_some() {
            ui_ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        for (i, (p, _)) in handles.iter().enumerate() {
            let is_hover = hovered_idx == Some(i);
            let halo_alpha = if is_hover { 80 } else { 40 };
            painter.circle_filled(*p, hr + 3.0 * zoom,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, halo_alpha));
            let fill = if is_hover { theme::TEXT_BRIGHT } else { theme::TEXT };
            painter.circle_filled(*p, hr, fill);
            painter.circle_stroke(*p, hr, Stroke::new(1.5 * zoom.max(0.5), theme::BG_DEEP));
        }

        if let Some(resp) = response
            && (resp.dragged() || resp.clicked())
            && let Some(pos) = resp.interact_pointer_pos()
        {
            if let Some((idx, _)) = handles.iter().enumerate()
                .filter(|(_, (p, _))| p.distance(pos) < hit_radius)
                .min_by(|(_, (a, _)), (_, (b, _))| a.distance(pos).partial_cmp(&b.distance(pos)).unwrap())
            {
                ui_ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                let (_, handle_id) = &handles[idx];
                let mut shared = self.shared.lock().unwrap();
                handle_id.apply_drag(pos, rect, &mut shared);
            }
        }
    }
}

/// A handle's behavior on drag — keeps each mode's geometry local.
#[derive(Clone, Copy)]
enum HandleId {
    // ADSR handles — the original interactive layout.
    /// Attack handle. Args: a_w, env_width_frac, total_time.
    Attack(f32, f32, f32),
    /// Decay+Sustain handle. Args: x1, a_w, d_w, env_width_frac, total, y_top, y_bot.
    DecaySustain(f32, f32, f32, f32, f32, f32, f32),
    /// Release handle. Args: x3, r_w, env_width_frac, total.
    Release(f32, f32, f32, f32),

    // AD handles.
    AdAttack(f32, f32, f32), // a_w, total, w
    AdDecay(f32, f32, f32, f32), // x1, d_w, total, w

    // AR handles.
    ArAttack(f32, f32, f32, f32), // a_w, env_width_frac, total, w
    ArRelease(f32, f32, f32, f32, f32), // x2, r_w, env_width_frac, total, w
}

impl HandleId {
    fn apply_drag(self, pos: Pos2, rect: egui::Rect, shared: &mut NodeSharedState) {
        let w = rect.width();
        let x0 = rect.min.x;
        match self {
            HandleId::Attack(_a_w, env_width_frac, total) => {
                let new_attack = ((pos.x - x0) / w * (total / env_width_frac)).clamp(0.001, A_MAX);
                shared.pending_params.push((0, ParamValue::Float(new_attack)));
            }
            HandleId::DecaySustain(x1, _a_w, _d_w, env_width_frac, total, y_top, y_bot) => {
                let new_decay = ((pos.x - x1) / w * (total / env_width_frac)).clamp(0.001, D_MAX);
                let new_sustain = 1.0 - ((pos.y - y_top) / (y_bot - y_top)).clamp(0.0, 1.0);
                shared.pending_params.push((1, ParamValue::Float(new_decay)));
                shared.pending_params.push((2, ParamValue::Float(new_sustain)));
            }
            HandleId::Release(x3, _r_w, env_width_frac, total) => {
                let new_release = ((pos.x - x3) / w * (total / env_width_frac)).clamp(0.001, R_MAX);
                shared.pending_params.push((3, ParamValue::Float(new_release)));
            }
            HandleId::AdAttack(_a_w, total, _w_full) => {
                let new_attack = ((pos.x - x0) / w * total).clamp(0.001, A_MAX);
                shared.pending_params.push((0, ParamValue::Float(new_attack)));
            }
            HandleId::AdDecay(x1, _d_w, total, _w_full) => {
                let new_decay = ((pos.x - x1) / w * total).clamp(0.001, D_MAX);
                shared.pending_params.push((1, ParamValue::Float(new_decay)));
            }
            HandleId::ArAttack(_a_w, env_width_frac, total, _w_full) => {
                let new_attack = ((pos.x - x0) / w * (total / env_width_frac)).clamp(0.001, A_MAX);
                shared.pending_params.push((0, ParamValue::Float(new_attack)));
            }
            HandleId::ArRelease(x2, _r_w, env_width_frac, total, _w_full) => {
                let new_release = ((pos.x - x2) / w * (total / env_width_frac)).clamp(0.001, R_MAX);
                shared.pending_params.push((3, ParamValue::Float(new_release)));
            }
        }
    }
}
