use std::any::Any;

use egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

use crate::engine::nodes::ui::peak_meter::{PeakMeterDisplay, PeakMeterOrientation};
use crate::engine::types::*;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

// ---- visual / behavior constants ------------------------------------------

const FLOOR_DB: f32 = -60.0;
const GREEN_TO_YELLOW_DB: f32 = -18.0;
const YELLOW_TO_RED_DB: f32 = -6.0;
/// Bar fills are painted up to this dB position (1.0 == 0 dB; values above
/// 0 dB are clip territory and just stay pegged at the top).
const CLIP_THRESHOLD: f32 = 0.999;
const PEAK_HOLD_DURATION: f64 = 1.5;
/// Peak-hold drop rate (units of normalized 0..1 per second) once the hold
/// timer expires.
const PEAK_HOLD_FALLBACK_RATE: f32 = 0.4;
const CLIP_DISPLAY_DURATION: f64 = 1.5;

const BG_COLOR: Color32 = Color32::from_gray(22);
const BORDER_COLOR: Color32 = Color32::from_gray(60);
const GREEN_COLOR: Color32 = Color32::from_rgb(80, 200, 100);
const YELLOW_COLOR: Color32 = Color32::from_rgb(230, 200, 60);
const RED_COLOR: Color32 = Color32::from_rgb(230, 70, 60);
const HOLD_COLOR: Color32 = Color32::from_rgb(220, 220, 220);
// Dark gray at ~50% alpha — premultiplied (40 * 128/255 ≈ 20).
const RMS_COLOR: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 128);
const CLIP_OFF_COLOR: Color32 = Color32::from_gray(40);
const SCALE_LABEL_COLOR: Color32 = Color32::from_gray(140);
const SCALE_TICK_COLOR: Color32 = Color32::from_gray(70);

const DB_TICKS: &[i32] = &[0, -3, -6, -12, -18, -24, -36, -48, -60];

// ---- helpers ---------------------------------------------------------------

fn level_to_db(level: f32) -> f32 {
    20.0 * level.max(1e-6).log10()
}

/// Map a dB value to a 0..1 vertical position (0 = bottom = silence,
/// 1 = top = 0 dB).
fn db_to_pos(db: f32) -> f32 {
    if db <= FLOOR_DB { return 0.0; }
    if db >= 0.0 { return 1.0; }
    1.0 - db / FLOOR_DB
}

fn level_to_pos(level: f32) -> f32 {
    db_to_pos(level_to_db(level))
}

// ---- widget ----------------------------------------------------------------

pub struct PeakMeterWidget {
    id: NodeId,
    shared: SharedState,
    name: String,
    /// Current peak (input port 0).
    peak: f32,
    /// Current RMS (input port 1, optional).
    rms: f32,
    orientation: PeakMeterOrientation,

    // UI-only state, not persisted.
    peak_hold: f32,
    peak_hold_time: f64,
    clip_time: Option<f64>,
}

impl PeakMeterWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id, shared,
            name: String::new(),
            peak: 0.0, rms: 0.0,
            orientation: PeakMeterOrientation::Vertical,
            peak_hold: 0.0,
            peak_hold_time: 0.0,
            clip_time: None,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({
            "name": self.name,
            "orientation": self.orientation.as_str(),
        }));
    }
}

impl NodeWidget for PeakMeterWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Peak Level Meter" }
    fn title(&self) -> &str {
        if self.name.is_empty() { "Peak Meter" } else { self.name.as_str() }
    }
    fn description(&self) -> &'static str {
        "Level meter: green/yellow/red dB scale, RMS overlay, peak hold and clip indicator."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![
            UiPortDef::from_def(&PortDef::new("peak", PortType::Untyped)),
            UiPortDef::from_def(&PortDef::new("rms", PortType::Untyped)),
        ]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> { vec![] }

    fn min_width(&self) -> f32 { 80.0 }
    fn min_content_height(&self) -> f32 { 100.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Sync from engine display.
        {
            let shared = self.shared.lock().unwrap();
            if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<PeakMeterDisplay>()) {
                self.name = d.name.clone();
                self.peak = d.peak;
                self.rms = d.rms;
                self.orientation = d.orientation;
            }
        }

        let now = ui.ctx().input(|i| i.time);

        // Update peak-hold tracker.
        if self.peak > self.peak_hold {
            self.peak_hold = self.peak;
            self.peak_hold_time = now;
        } else {
            let elapsed = now - self.peak_hold_time;
            if elapsed > PEAK_HOLD_DURATION {
                let decay = PEAK_HOLD_FALLBACK_RATE * (elapsed - PEAK_HOLD_DURATION) as f32;
                self.peak_hold = (self.peak_hold - decay).max(self.peak);
            }
        }

        // Clip detection.
        if self.peak >= CLIP_THRESHOLD {
            self.clip_time = Some(now);
        }

        let avail = ui.available_size();
        if avail.x <= 0.0 || avail.y <= 0.0 { return; }

        let (resp, painter) = ui.allocate_painter(avail, Sense::click());
        let rect = resp.rect;

        // Click on the clip indicator area resets it.
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if pos.y < rect.min.y + 14.0 {
                    self.clip_time = None;
                }
            }
        }

        match self.orientation {
            PeakMeterOrientation::Vertical => draw_vertical(
                &painter, rect, self.peak, self.rms, self.peak_hold,
                clip_active(self.clip_time, now),
            ),
            PeakMeterOrientation::Horizontal => draw_horizontal(
                &painter, rect, self.peak, self.rms, self.peak_hold,
                clip_active(self.clip_time, now),
            ),
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Name:");
            if ui.text_edit_singleline(&mut self.name).changed() {
                changed = true;
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Orientation:");
            if ui.radio_value(&mut self.orientation, PeakMeterOrientation::Vertical, "Vertical").clicked() {
                changed = true;
            }
            if ui.radio_value(&mut self.orientation, PeakMeterOrientation::Horizontal, "Horizontal").clicked() {
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Peak:");
            ui.colored_label(Color32::from_gray(200), format!("{:.3} ({:+.1} dB)", self.peak, level_to_db(self.peak)));
        });
        ui.horizontal(|ui| {
            ui.label("RMS:");
            ui.colored_label(Color32::from_gray(200), format!("{:.3} ({:+.1} dB)", self.rms, level_to_db(self.rms)));
        });
        ui.horizontal(|ui| {
            if ui.small_button("Reset clip").clicked() {
                self.clip_time = None;
            }
            if ui.small_button("Reset hold").clicked() {
                self.peak_hold = 0.0;
            }
        });
        if changed { self.push_config(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

pub fn clip_active(clip_time: Option<f64>, now: f64) -> bool {
    clip_time.map(|t| now - t < CLIP_DISPLAY_DURATION).unwrap_or(false)
}

/// Per-meter UI state used to drive peak-hold and clip indicator. Owned by
/// the host widget; updated each frame via `tick`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeterState {
    pub peak_hold: f32,
    pub peak_hold_time: f64,
    pub clip_time: Option<f64>,
}

impl MeterState {
    /// Update peak-hold tracker and clip latch from the current peak value.
    /// Call once per frame before drawing.
    pub fn tick(&mut self, peak: f32, now: f64) {
        if peak > self.peak_hold {
            self.peak_hold = peak;
            self.peak_hold_time = now;
        } else {
            let elapsed = now - self.peak_hold_time;
            if elapsed > PEAK_HOLD_DURATION {
                let decay = PEAK_HOLD_FALLBACK_RATE * (elapsed - PEAK_HOLD_DURATION) as f32;
                self.peak_hold = (self.peak_hold - decay).max(peak);
            }
        }
        if peak >= CLIP_THRESHOLD {
            self.clip_time = Some(now);
        }
    }

    pub fn clipping(&self, now: f64) -> bool {
        clip_active(self.clip_time, now)
    }
}

// ---- drawing ---------------------------------------------------------------

fn draw_vertical(
    painter: &egui::Painter,
    rect: Rect,
    peak: f32,
    rms: f32,
    peak_hold: f32,
    clipping: bool,
) {
    // Layout regions: clip indicator at top (14px), then dB-scale labels on
    // the left (20px), bar to the right.
    let clip_h = 14.0;
    let scale_w = if rect.width() >= 50.0 { 22.0 } else { 0.0 };

    let clip_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + scale_w, rect.min.y),
        Pos2::new(rect.max.x, rect.min.y + clip_h),
    );
    let bar_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + scale_w, rect.min.y + clip_h + 2.0),
        Pos2::new(rect.max.x, rect.max.y),
    );

    // Clip indicator (top).
    painter.rect_filled(clip_rect, 2.0, if clipping { RED_COLOR } else { CLIP_OFF_COLOR });
    painter.text(
        clip_rect.center(),
        egui::Align2::CENTER_CENTER,
        "CLIP",
        egui::FontId::proportional(9.0),
        if clipping { Color32::BLACK } else { Color32::from_gray(120) },
    );

    // Bar background + border.
    painter.rect_filled(bar_rect, 1.0, BG_COLOR);

    // Zoned color fill for peak.
    let h = bar_rect.height();
    let bottom_y = bar_rect.max.y;
    let level_pos = level_to_pos(peak);
    let g_y_pos = db_to_pos(GREEN_TO_YELLOW_DB);
    let y_r_pos = db_to_pos(YELLOW_TO_RED_DB);

    fill_band(painter, bar_rect, bottom_y, h, 0.0, g_y_pos.min(level_pos), GREEN_COLOR);
    if level_pos > g_y_pos {
        fill_band(painter, bar_rect, bottom_y, h, g_y_pos, y_r_pos.min(level_pos), YELLOW_COLOR);
    }
    if level_pos > y_r_pos {
        fill_band(painter, bar_rect, bottom_y, h, y_r_pos, level_pos, RED_COLOR);
    }

    // RMS overlay (semi-transparent white bar).
    let rms_pos = level_to_pos(rms);
    if rms_pos > 0.0 {
        let rms_rect = Rect::from_min_max(
            Pos2::new(bar_rect.min.x, bottom_y - rms_pos * h),
            Pos2::new(bar_rect.max.x, bottom_y),
        );
        painter.rect_filled(rms_rect, 0.0, RMS_COLOR);
    }

    // Peak hold line.
    let hold_pos = level_to_pos(peak_hold);
    if hold_pos > 0.0 {
        let y = bottom_y - hold_pos * h;
        painter.line_segment(
            [Pos2::new(bar_rect.min.x, y), Pos2::new(bar_rect.max.x, y)],
            Stroke::new(1.5, HOLD_COLOR),
        );
    }

    painter.rect_stroke(bar_rect, 1.0, Stroke::new(1.0, BORDER_COLOR), StrokeKind::Inside);

    // dB scale labels + ticks (only if there's room).
    if scale_w > 0.0 {
        for &db in DB_TICKS {
            let pos = db_to_pos(db as f32);
            let y = bottom_y - pos * h;
            painter.line_segment(
                [Pos2::new(bar_rect.min.x - 3.0, y), Pos2::new(bar_rect.min.x, y)],
                Stroke::new(1.0, SCALE_TICK_COLOR),
            );
            painter.text(
                Pos2::new(bar_rect.min.x - 4.0, y),
                egui::Align2::RIGHT_CENTER,
                if db == 0 { "0".to_string() } else { format!("{}", db) },
                egui::FontId::monospace(8.0),
                SCALE_LABEL_COLOR,
            );
        }
    }
}

fn fill_band(
    painter: &egui::Painter,
    rect: Rect,
    bottom_y: f32,
    h: f32,
    from_pos: f32,
    to_pos: f32,
    color: Color32,
) {
    if to_pos <= from_pos { return; }
    let band_rect = Rect::from_min_max(
        Pos2::new(rect.min.x, bottom_y - to_pos * h),
        Pos2::new(rect.max.x, bottom_y - from_pos * h),
    );
    painter.rect_filled(band_rect, 0.0, color);
}

pub fn draw_horizontal(
    painter: &egui::Painter,
    rect: Rect,
    peak: f32,
    rms: f32,
    peak_hold: f32,
    clipping: bool,
) {
    // Clip indicator on the right edge (14px wide).
    let clip_w = 14.0;
    let bar_rect = Rect::from_min_max(
        rect.min,
        Pos2::new(rect.max.x - clip_w - 2.0, rect.max.y),
    );
    let clip_rect = Rect::from_min_max(
        Pos2::new(rect.max.x - clip_w, rect.min.y),
        rect.max,
    );

    // Clip indicator.
    painter.rect_filled(clip_rect, 2.0, if clipping { RED_COLOR } else { CLIP_OFF_COLOR });
    painter.text(
        clip_rect.center(),
        egui::Align2::CENTER_CENTER,
        "C",
        egui::FontId::proportional(9.0),
        if clipping { Color32::BLACK } else { Color32::from_gray(120) },
    );

    // Background.
    painter.rect_filled(bar_rect, 1.0, BG_COLOR);

    let w = bar_rect.width();
    let left_x = bar_rect.min.x;
    let level_pos = level_to_pos(peak);
    let g_y_pos = db_to_pos(GREEN_TO_YELLOW_DB);
    let y_r_pos = db_to_pos(YELLOW_TO_RED_DB);

    fill_band_h(painter, bar_rect, left_x, w, 0.0, g_y_pos.min(level_pos), GREEN_COLOR);
    if level_pos > g_y_pos {
        fill_band_h(painter, bar_rect, left_x, w, g_y_pos, y_r_pos.min(level_pos), YELLOW_COLOR);
    }
    if level_pos > y_r_pos {
        fill_band_h(painter, bar_rect, left_x, w, y_r_pos, level_pos, RED_COLOR);
    }

    // RMS overlay.
    let rms_pos = level_to_pos(rms);
    if rms_pos > 0.0 {
        let rms_rect = Rect::from_min_max(
            bar_rect.min,
            Pos2::new(left_x + rms_pos * w, bar_rect.max.y),
        );
        painter.rect_filled(rms_rect, 0.0, RMS_COLOR);
    }

    // Peak hold line.
    let hold_pos = level_to_pos(peak_hold);
    if hold_pos > 0.0 {
        let x = left_x + hold_pos * w;
        painter.line_segment(
            [Pos2::new(x, bar_rect.min.y), Pos2::new(x, bar_rect.max.y)],
            Stroke::new(1.5, HOLD_COLOR),
        );
    }

    painter.rect_stroke(bar_rect, 1.0, Stroke::new(1.0, BORDER_COLOR), StrokeKind::Inside);
}

fn fill_band_h(
    painter: &egui::Painter,
    rect: Rect,
    left_x: f32,
    w: f32,
    from_pos: f32,
    to_pos: f32,
    color: Color32,
) {
    if to_pos <= from_pos { return; }
    let band_rect = Rect::from_min_max(
        Pos2::new(left_x + from_pos * w, rect.min.y),
        Pos2::new(left_x + to_pos * w, rect.max.y),
    );
    painter.rect_filled(band_rect, 0.0, color);
}
