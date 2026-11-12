use std::any::Any;

use egui::{self, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::engine::nodes::transport::envelope::{EnvelopeDisplay, EnvelopeKind, curve_for_mid, shape};
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
    /// What the pointer took hold of, and the time scale (pixels per second)
    /// frozen at that moment: the scale follows the total length, so without
    /// freezing it the point would run away from the pointer as it is dragged.
    grab: Option<(Grab, f32)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Grab {
    /// A point of the curve, by index into `Geo::pts`.
    Point(usize),
    /// A segment, by index into `Geo::segs`; dragging bends it.
    Segment(usize),
}

impl EnvelopeWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, kind: EnvelopeKind::Adsr, grab: None }
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
        // 0=Attack/Rise/Tau, 1=Decay, 2=Sustain, 3=Release/Fall, 4=Mode, 5-7 the
        // bend of attack, decay and release.
        match self.kind {
            EnvelopeKind::Adsr => vec![],
            EnvelopeKind::Ad => vec![2, 3, 7],
            EnvelopeKind::Ar => vec![1, 2, 6],
            EnvelopeKind::Follower => vec![1, 2, 3, 5, 6, 7],
            EnvelopeKind::FollowerRf => vec![1, 2, 5, 6, 7],
            EnvelopeKind::SlopeUp => vec![1, 2, 3, 5, 6, 7],
            EnvelopeKind::SlopeDown => vec![0, 1, 2, 5, 6, 7],
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
        let (kind, stage, env_val, times) = if let Some(d) = display {
            (d.kind, d.stage, d.envelope_value, Times {
                attack: d.attack, decay: d.decay, sustain: d.sustain, release: d.release,
                attack_curve: d.attack_curve, decay_curve: d.decay_curve, release_curve: d.release_curve,
            })
        } else {
            (self.kind, 0, 0.0, Times {
                attack: 0.02, decay: 0.08, sustain: 0.6, release: 0.15,
                attack_curve: 0.0, decay_curve: 0.0, release_curve: 0.0,
            })
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
            EnvelopeKind::Adsr | EnvelopeKind::Ad | EnvelopeKind::Ar =>
                self.curve_editor(ui, &painter, &response, rect, zoom, kind, stage, env_val, times),
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
// Curve editor (ADSR / AD / AR)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Times {
    attack: f32, decay: f32, sustain: f32, release: f32,
    attack_curve: f32, decay_curve: f32, release_curve: f32,
}

#[derive(Clone, Copy)]
struct Seg {
    a: usize,
    b: usize,
    /// Parameter index of this segment's bend; None for the flat hold.
    curve_param: Option<usize>,
    curve: f32,
}

struct Geo {
    pts: Vec<Pos2>,
    segs: Vec<Seg>,
    /// Indices into `pts` that can be dragged.
    handles: Vec<usize>,
    scale: f32,
    y_top: f32,
    y_bot: f32,
}

const CURVE_SAMPLES: usize = 24;

impl Geo {
    fn of(kind: EnvelopeKind, rect: egui::Rect, t: Times, scale_lock: Option<f32>) -> Geo {
        let w = rect.width();
        let x0 = rect.min.x;
        let y_bot = rect.max.y - 2.0;
        let y_top = rect.min.y + 2.0;
        let a = t.attack.min(A_MAX);
        let d = t.decay.min(D_MAX);
        let r = t.release.min(R_MAX);
        let cx = |x: f32| x.min(rect.max.x);
        let seg = |a, b, p, c| Seg { a, b, curve_param: p, curve: c };
        match kind {
            EnvelopeKind::Adsr => {
                let total = (a + d + r).max(0.001);
                let scale = scale_lock.unwrap_or(0.8 * w / total);
                let y_sus = y_bot - t.sustain * (y_bot - y_top);
                let x1 = x0 + a * scale;
                let x2 = x1 + d * scale;
                let x3 = x2 + 0.2 * w;
                let x4 = x3 + r * scale;
                Geo {
                    pts: vec![
                        Pos2::new(x0, y_bot), Pos2::new(cx(x1), y_top), Pos2::new(cx(x2), y_sus),
                        Pos2::new(cx(x3), y_sus), Pos2::new(cx(x4), y_bot),
                    ],
                    segs: vec![
                        seg(0, 1, Some(5), t.attack_curve), seg(1, 2, Some(6), t.decay_curve),
                        seg(2, 3, None, 0.0), seg(3, 4, Some(7), t.release_curve),
                    ],
                    handles: vec![1, 2, 4], scale, y_top, y_bot,
                }
            }
            EnvelopeKind::Ad => {
                let total = (a + d).max(0.001);
                let scale = scale_lock.unwrap_or(w / total);
                let x1 = x0 + a * scale;
                let x2 = x1 + d * scale;
                Geo {
                    pts: vec![Pos2::new(x0, y_bot), Pos2::new(cx(x1), y_top), Pos2::new(cx(x2), y_bot)],
                    segs: vec![seg(0, 1, Some(5), t.attack_curve), seg(1, 2, Some(6), t.decay_curve)],
                    handles: vec![1, 2], scale, y_top, y_bot,
                }
            }
            _ => {
                let total = (a + r).max(0.001);
                let scale = scale_lock.unwrap_or(0.7 * w / total);
                let x1 = x0 + a * scale;
                let x2 = x1 + 0.3 * w;
                let x3 = x2 + r * scale;
                Geo {
                    pts: vec![
                        Pos2::new(x0, y_bot), Pos2::new(cx(x1), y_top), Pos2::new(cx(x2), y_top),
                        Pos2::new(cx(x3), y_bot),
                    ],
                    segs: vec![
                        seg(0, 1, Some(5), t.attack_curve), seg(1, 2, None, 0.0),
                        seg(2, 3, Some(7), t.release_curve),
                    ],
                    handles: vec![1, 3], scale, y_top, y_bot,
                }
            }
        }
    }

    /// Points along a segment, drawn with the very function the engine runs.
    fn polyline(&self, s: Seg) -> Vec<Pos2> {
        let (pa, pb) = (self.pts[s.a], self.pts[s.b]);
        (0..=CURVE_SAMPLES).map(|i| {
            let t = i as f32 / CURVE_SAMPLES as f32;
            Pos2::new(pa.x + (pb.x - pa.x) * t, pa.y + (pb.y - pa.y) * shape(t, s.curve))
        }).collect()
    }

    fn pick(&self, pos: Pos2, hit: f32) -> Option<Grab> {
        let point = self.handles.iter().copied()
            .map(|i| (i, self.pts[i].distance(pos)))
            .filter(|(_, d)| *d < hit)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(i, _)| Grab::Point(i));
        if point.is_some() {
            return point;
        }
        self.segs.iter().enumerate()
            .filter(|(_, s)| s.curve_param.is_some())
            .map(|(i, s)| {
                let line = self.polyline(*s);
                let d = line.windows(2)
                    .map(|p| dist_to_segment(pos, p[0], p[1]))
                    .fold(f32::MAX, f32::min);
                (i, d)
            })
            .filter(|(_, d)| *d < hit * 0.7)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(i, _)| Grab::Segment(i))
    }
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        return p.distance(a);
    }
    let t = (((p - a).dot(ab)) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Time fraction at which `shape` has reached `f`; the engine only knows the
/// height, and the running dot is drawn by where the curve is at that height.
fn unshape(f: f32, curve: f32) -> f32 {
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if shape(mid, curve) < f { lo = mid } else { hi = mid }
    }
    0.5 * (lo + hi)
}

impl EnvelopeWidget {
    #[allow(clippy::too_many_arguments)]
    fn curve_editor(
        &mut self, ui: &Ui, painter: &egui::Painter, resp: &egui::Response, rect: egui::Rect,
        zoom: f32, kind: EnvelopeKind, stage: u8, env_val: f32, times: Times,
    ) {
        let lock = self.grab.map(|(_, s)| s);
        let geo = Geo::of(kind, rect, times, lock);
        let hr = HANDLE_RADIUS * zoom;
        let hit = hr + 8.0 * zoom;

        // Gestures: press on a point to move it, on a line to bend it; a
        // double-click on a line straightens it. The grab is kept until the
        // button goes up, wherever the pointer wanders.
        if resp.drag_started()
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        {
            self.grab = geo.pick(origin, hit).map(|g| (g, geo.scale));
        }
        if resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && let Some(Grab::Segment(i)) = geo.pick(pos, hit)
            && let Some(param) = geo.segs[i].curve_param
        {
            self.shared.lock().unwrap().pending_params.push((param, ParamValue::Float(0.0)));
        }
        if let Some((grab, _)) = self.grab {
            if resp.dragged() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    self.apply_drag(kind, &geo, grab, pos, rect);
                }
            } else {
                self.grab = None;
            }
        }

        let hover = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
        let hot = self.grab.map(|(g, _)| g).or_else(|| hover.and_then(|p| geo.pick(p, hit)));
        if self.grab.is_none() {
            match hot {
                Some(Grab::Point(_)) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
                Some(Grab::Segment(_)) => ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical),
                None => {}
            }
        }

        // Fill under the curve as a mesh of triangles: the shape is not convex.
        let fill = theme::SEM_SUCCESS.linear_multiply(0.15);
        let mut mesh = egui::Mesh::default();
        for s in &geo.segs {
            let line = geo.polyline(*s);
            for p in line.windows(2) {
                let base = mesh.vertices.len() as u32;
                for pos in [p[0], p[1], Pos2::new(p[1].x, geo.y_bot), Pos2::new(p[0].x, geo.y_bot)] {
                    mesh.colored_vertex(pos, fill);
                }
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base, base + 2, base + 3);
            }
        }
        painter.add(egui::Shape::mesh(mesh));

        for (i, s) in geo.segs.iter().enumerate() {
            let lit = hot == Some(Grab::Segment(i));
            let mut color = if s.curve_param.is_some() { theme::SEM_SUCCESS } else { theme::SEM_SUCCESS.linear_multiply(0.6) };
            if lit { color = theme::TEXT_BRIGHT; }
            painter.add(egui::Shape::line(geo.polyline(*s), Stroke::new(2.0_f32, color)));
        }

        if stage > 0
            && let Some((x, y)) = self.indicator(kind, &geo, stage, env_val, times)
        {
            painter.circle_filled(Pos2::new(x, y), hr, theme::TEXT_BRIGHT);
        }

        for &i in &geo.handles {
            let p = geo.pts[i];
            let is_hot = hot == Some(Grab::Point(i));
            let halo = if is_hot { 80 } else { 40 };
            painter.circle_filled(p, hr + 3.0 * zoom, egui::Color32::from_rgba_unmultiplied(255, 255, 255, halo));
            painter.circle_filled(p, hr, if is_hot { theme::TEXT_BRIGHT } else { theme::TEXT });
            painter.circle_stroke(p, hr, Stroke::new(1.5 * zoom.max(0.5), theme::BG_DEEP));
        }
    }

    /// Where the running dot sits on the drawn curve for the current stage.
    fn indicator(&self, kind: EnvelopeKind, geo: &Geo, stage: u8, env: f32, t: Times) -> Option<(f32, f32)> {
        let (seg, from, to) = match (kind, stage) {
            (_, 1) => (0, 0.0, 1.0),
            (EnvelopeKind::Adsr, 2) => (1, 1.0, t.sustain),
            (EnvelopeKind::Ad, 2) => (1, 1.0, 0.0),
            (EnvelopeKind::Adsr, 3) => {
                let (a, b) = (geo.pts[2], geo.pts[3]);
                return Some((0.5 * (a.x + b.x), a.y));
            }
            (EnvelopeKind::Ar, 3) => {
                let (a, b) = (geo.pts[1], geo.pts[2]);
                return Some((0.5 * (a.x + b.x), a.y));
            }
            (EnvelopeKind::Adsr, 4) => (3, t.sustain, 0.0),
            (EnvelopeKind::Ar, 4) => (2, 1.0, 0.0),
            _ => return None,
        };
        let s = geo.segs[seg];
        let span = to - from;
        let f = if span.abs() < 1e-4 { 1.0 } else { ((env - from) / span).clamp(0.0, 1.0) };
        let tt = unshape(f, s.curve);
        let (pa, pb) = (geo.pts[s.a], geo.pts[s.b]);
        Some((pa.x + (pb.x - pa.x) * tt, pa.y + (pb.y - pa.y) * f))
    }

    fn apply_drag(&self, kind: EnvelopeKind, geo: &Geo, grab: Grab, pos: Pos2, rect: egui::Rect) {
        let mut shared = self.shared.lock().unwrap();
        let mut set = |idx: usize, v: f32| shared.pending_params.push((idx, ParamValue::Float(v)));
        let x = pos.x.clamp(rect.min.x, rect.max.x);
        match grab {
            Grab::Segment(i) => {
                let s = geo.segs[i];
                let (ya, yb) = (geo.pts[s.a].y, geo.pts[s.b].y);
                if let Some(param) = s.curve_param
                    && (yb - ya).abs() > 1.0
                {
                    set(param, curve_for_mid((pos.y - ya) / (yb - ya)));
                }
            }
            Grab::Point(i) => {
                let time = |from_x: f32, max: f32| ((x - from_x) / geo.scale).clamp(0.001, max);
                match (kind, i) {
                    (_, 1) => set(0, time(geo.pts[0].x, A_MAX)),
                    (EnvelopeKind::Adsr, 2) => {
                        set(1, time(geo.pts[1].x, D_MAX));
                        set(2, 1.0 - ((pos.y - geo.y_top) / (geo.y_bot - geo.y_top)).clamp(0.0, 1.0));
                    }
                    (EnvelopeKind::Adsr, 4) => set(3, time(geo.pts[3].x, R_MAX)),
                    (EnvelopeKind::Ad, 2) => set(1, time(geo.pts[1].x, D_MAX)),
                    (EnvelopeKind::Ar, 3) => set(3, time(geo.pts[2].x, R_MAX)),
                    _ => {}
                }
            }
        }
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
}
