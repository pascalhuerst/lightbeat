use std::any::Any;
use std::sync::{Arc, Mutex};

use egui::{self, Color32, Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::color::{GradientStop, Rgb};
use crate::engine::nodes::math::gradient_source::GradientSourceDisplay;
use crate::engine::types::*;
use crate::objects::gradient_preset::GradientPreset;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

const MAX_STOPS: usize = GRADIENT_STOP_COUNT;

/// A "save current stops as a new preset" request the widget pushes when
/// the user clicks Save preset. main.rs drains these each frame and asks
/// the GradientPresetManager to assign an id and store the entry, then
/// re-syncs the shared library mirror.
#[derive(Clone)]
pub struct PendingPresetSave {
    pub name: String,
    pub stops: Vec<GradientStop>,
}

/// Read-only mirror of the gradient preset library plus a write-back queue
/// for "Save current as preset" requests from any Gradient Source widget.
#[derive(Default)]
pub struct GradientLibraryContext {
    pub presets: Vec<GradientPreset>,
    pub pending_saves: Vec<PendingPresetSave>,
}

pub type SharedGradientLibrary = Arc<Mutex<GradientLibraryContext>>;

pub fn new_shared_gradient_library() -> SharedGradientLibrary {
    Arc::new(Mutex::new(GradientLibraryContext::default()))
}

pub struct GradientSourceWidget {
    id: NodeId,
    shared: SharedState,
    /// Library handle — shared across all Gradient Source instances and the
    /// Gradients management window. Read for the dropdown, written via
    /// `pending_saves` for the save button.
    library: SharedGradientLibrary,
    /// `(used, position, r, g, b, alpha)` per slot. The widget is the
    /// authoritative source of truth for edits; on change it pushes to
    /// pending_config which the engine's load_data consumes.
    stops: [StopEdit; MAX_STOPS],
    /// Mirror of the engine's latest active stops — used by the node
    /// preview so the visual matches what's being emitted.
    preview_stops: Vec<(f32, egui::Color32, f32)>,
    /// Buffer for the inline "Save preset" name field.
    save_name_buf: String,
}

#[derive(Clone, Copy)]
struct StopEdit {
    used: bool,
    position: f32,
    r: f32,
    g: f32,
    b: f32,
    alpha: f32,
}

impl Default for StopEdit {
    fn default() -> Self {
        Self { used: false, position: 0.0, r: 0.0, g: 0.0, b: 0.0, alpha: 1.0 }
    }
}

impl GradientSourceWidget {
    pub fn new(id: NodeId, shared: SharedState, library: SharedGradientLibrary) -> Self {
        // Default: black → white two-stop gradient (matches the engine's default).
        let mut stops = [StopEdit::default(); MAX_STOPS];
        stops[0] = StopEdit { used: true, position: 0.0, r: 0.0, g: 0.0, b: 0.0, alpha: 1.0 };
        stops[1] = StopEdit { used: true, position: 1.0, r: 1.0, g: 1.0, b: 1.0, alpha: 1.0 };
        Self {
            id, shared, library, stops,
            preview_stops: Vec::new(),
            save_name_buf: String::new(),
        }
    }

    /// Replace the current stops with the given preset's stops and push to
    /// the engine.
    fn load_from_stops(&mut self, src: &[GradientStop]) {
        self.stops = [StopEdit::default(); MAX_STOPS];
        for (i, s) in src.iter().take(MAX_STOPS).enumerate() {
            self.stops[i] = StopEdit {
                used: true,
                position: s.position.clamp(0.0, 1.0),
                r: s.color.r.clamp(0.0, 1.0),
                g: s.color.g.clamp(0.0, 1.0),
                b: s.color.b.clamp(0.0, 1.0),
                alpha: s.alpha.clamp(0.0, 1.0),
            };
        }
        self.push_config();
    }

    /// Snapshot the current widget stops into a sorted `Vec<GradientStop>`
    /// suitable for storing in the library.
    fn current_stops(&self) -> Vec<GradientStop> {
        let mut out: Vec<GradientStop> = self.stops.iter()
            .filter(|s| s.used)
            .map(|s| GradientStop {
                position: s.position.clamp(0.0, 1.0),
                color: Rgb::new(
                    s.r.clamp(0.0, 1.0),
                    s.g.clamp(0.0, 1.0),
                    s.b.clamp(0.0, 1.0),
                ),
                alpha: s.alpha.clamp(0.0, 1.0),
            })
            .collect();
        // Gradient::new sorts on construct; do it ourselves so the saved
        // order matches what samplers see.
        out.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    fn push_config(&self) {
        let stops: Vec<serde_json::Value> = self.stops.iter().map(|s| {
            serde_json::json!({
                "used": s.used,
                "position": s.position,
                "r": s.r, "g": s.g, "b": s.b,
                "alpha": s.alpha,
            })
        }).collect();
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({ "stops": stops }));
    }

    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(arr) = data.get("stops").and_then(|v| v.as_array()) {
            for (i, entry) in arr.iter().take(MAX_STOPS).enumerate() {
                let used = entry.get("used").and_then(|v| v.as_bool()).unwrap_or(false);
                if !used { self.stops[i] = StopEdit::default(); continue; }
                self.stops[i] = StopEdit {
                    used: true,
                    position: entry.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    r: entry.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    g: entry.get("g").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    b: entry.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    alpha: entry.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                };
            }
        }
    }
}

impl NodeWidget for GradientSourceWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gradient Source" }
    fn title(&self) -> &str { "Gradient Source" }
    fn description(&self) -> &'static str {
        "Authors an 8-stop gradient (color + alpha + position per stop). Output feeds Group Output and any other Gradient-accepting node."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("gradient", PortType::Gradient))]
    }

    fn min_width(&self) -> f32 { 140.0 }
    fn min_content_height(&self) -> f32 { 40.0 }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        // Pull the active stops from the engine display so the node always
        // renders what's actually being emitted (including after load).
        let snap = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<GradientSourceDisplay>())
                .map(|d| d.stops.iter().map(|(p, c, a)| {
                    let col = egui::Color32::from_rgba_unmultiplied(
                        (c.r.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.g.clamp(0.0, 1.0) * 255.0) as u8,
                        (c.b.clamp(0.0, 1.0) * 255.0) as u8,
                        (a.clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    (*p, col, *a)
                }).collect::<Vec<_>>())
        };
        if let Some(s) = snap { self.preview_stops = s; }

        // Layout: gradient bar on top, draggable stop handles in a strip below.
        let avail = ui.available_size();
        const HANDLE_STRIP: f32 = 16.0;
        let bar_h = (avail.y - HANDLE_STRIP).max(20.0);
        let total_size = Vec2::new(avail.x, bar_h + HANDLE_STRIP);
        let (resp, painter) = ui.allocate_painter(total_size, Sense::click_and_drag());
        let bar_rect = egui::Rect::from_min_size(resp.rect.min, Vec2::new(resp.rect.width(), bar_h));

        // Checkerboard so alpha is visible.
        draw_checker(&painter, bar_rect);

        // Sample the gradient at many points across width.
        let samples = (bar_rect.width() as usize).max(16).min(512);
        if !self.preview_stops.is_empty() {
            for i in 0..samples {
                let t = i as f32 / (samples - 1).max(1) as f32;
                let x = bar_rect.min.x + (i as f32 / samples as f32) * bar_rect.width();
                let col = sample_preview(&self.preview_stops, t);
                painter.line_segment(
                    [Pos2::new(x, bar_rect.min.y), Pos2::new(x, bar_rect.max.y)],
                    Stroke::new(bar_rect.width() / samples as f32 + 0.5, col),
                );
            }
        } else {
            painter.rect_filled(bar_rect, 2.0, Color32::from_gray(40));
        }
        painter.rect_stroke(bar_rect, 2.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);

        // Double-click on the bar (not on a handle) creates a new stop at
        // the clicked position, sampled from the current gradient so the
        // visual stays continuous.
        let mut new_stop_at: Option<f32> = None;
        if resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
                && pos.y <= bar_rect.max.y {
                    let logical = ((pos.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                    new_stop_at = Some(logical);
                }

        // Draggable stop handles in the strip below the bar.
        let handle_w = 10.0;
        let handle_h = 12.0;
        let handle_y = bar_rect.max.y + HANDLE_STRIP * 0.5;
        let mut delete_idx: Option<usize> = None;
        let mut changed = false;
        let node_id = self.id.0;
        for (i, s) in self.stops.iter_mut().enumerate() {
            if !s.used { continue; }
            let x = bar_rect.min.x + s.position.clamp(0.0, 1.0) * bar_rect.width();
            let handle_rect = egui::Rect::from_center_size(
                Pos2::new(x, handle_y),
                Vec2::new(handle_w, handle_h),
            );
            let id = ui.id().with(("grad_stop", node_id, i));
            let h_resp = ui.interact(handle_rect, id, Sense::click_and_drag());

            if h_resp.dragged()
                && let Some(p) = h_resp.interact_pointer_pos() {
                    let new_pos = ((p.x - bar_rect.min.x) / bar_rect.width()).clamp(0.0, 1.0);
                    if (new_pos - s.position).abs() > 1e-4 {
                        s.position = new_pos;
                        changed = true;
                    }
                }
            if h_resp.secondary_clicked() {
                delete_idx = Some(i);
            }

            // Click (not drag) opens an inline color/alpha popup for this stop.
            let popup_id = ui.id().with(("grad_stop_popup", node_id, i));
            if h_resp.clicked() {
                ui.memory_mut(|m| m.open_popup(popup_id));
            }
            egui::popup::popup_below_widget(
                ui,
                popup_id,
                &h_resp,
                egui::popup::PopupCloseBehavior::CloseOnClickOutside,
                |ui| {
                    ui.set_min_width(220.0);
                    // Inline color picker (no nested popup, so it doesn't
                    // race with the outer popup's click-outside close).
                    let mut color = Color32::from_rgba_unmultiplied(
                        (s.r.clamp(0.0, 1.0) * 255.0) as u8,
                        (s.g.clamp(0.0, 1.0) * 255.0) as u8,
                        (s.b.clamp(0.0, 1.0) * 255.0) as u8,
                        (s.alpha.clamp(0.0, 1.0) * 255.0) as u8,
                    );
                    if egui::color_picker::color_picker_color32(
                        ui,
                        &mut color,
                        egui::color_picker::Alpha::OnlyBlend,
                    ) {
                        s.r = color.r() as f32 / 255.0;
                        s.g = color.g() as f32 / 255.0;
                        s.b = color.b() as f32 / 255.0;
                        s.alpha = color.a() as f32 / 255.0;
                        changed = true;
                    }
                    if ui.button(egui_phosphor::regular::X.to_string() + " Remove").clicked() {
                        delete_idx = Some(i);
                        ui.memory_mut(|m| m.close_popup());
                    }
                },
            );

            let fill = Color32::from_rgba_unmultiplied(
                (s.r.clamp(0.0, 1.0) * 255.0) as u8,
                (s.g.clamp(0.0, 1.0) * 255.0) as u8,
                (s.b.clamp(0.0, 1.0) * 255.0) as u8,
                (s.alpha.clamp(0.0, 1.0) * 255.0) as u8,
            );
            let stroke = if h_resp.hovered() || h_resp.dragged() {
                Stroke::new(1.5, Color32::WHITE)
            } else {
                Stroke::new(1.0, Color32::from_gray(120))
            };
            // Tick connecting handle to its position on the bar.
            painter.line_segment(
                [Pos2::new(x, bar_rect.max.y), Pos2::new(x, handle_rect.min.y)],
                Stroke::new(1.0, Color32::from_gray(140)),
            );
            painter.rect_filled(handle_rect, 2.0, fill);
            painter.rect_stroke(handle_rect, 2.0, stroke, StrokeKind::Inside);
        }

        if let Some(p) = new_stop_at {
            // Sample the current preview at the clicked position so the new
            // stop's color matches the existing gradient at that point.
            let sampled = sample_preview(&self.preview_stops, p);
            if let Some(slot) = self.stops.iter().position(|s| !s.used) {
                self.stops[slot] = StopEdit {
                    used: true,
                    position: p,
                    r: sampled.r() as f32 / 255.0,
                    g: sampled.g() as f32 / 255.0,
                    b: sampled.b() as f32 / 255.0,
                    alpha: sampled.a() as f32 / 255.0,
                };
                changed = true;
            }
        }

        if let Some(idx) = delete_idx {
            self.stops[idx] = StopEdit::default();
            changed = true;
        }

        if changed { self.push_config(); }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let mut swap: Option<(usize, usize)> = None;

        // -- Preset library: load and save -------------------------------
        ui.label(egui::RichText::new("Preset Library").strong());
        let presets: Vec<(u32, String, Vec<GradientStop>)> = {
            let lib = self.library.lock().unwrap();
            lib.presets.iter().map(|p| (p.id, p.name.clone(), p.stops.clone())).collect()
        };
        ui.horizontal(|ui| {
            ui.label("Load");
            let combo = egui::ComboBox::from_id_salt(("grad_preset_load", self.id.0))
                .selected_text("(pick…)");
            let mut load_stops: Option<Vec<GradientStop>> = None;
            combo.show_ui(ui, |ui| {
                if presets.is_empty() {
                    ui.colored_label(Color32::from_gray(120), "No presets yet");
                }
                for (_id, name, stops) in &presets {
                    if ui.selectable_label(false, name).clicked() {
                        load_stops = Some(stops.clone());
                    }
                }
            });
            if let Some(stops) = load_stops {
                self.load_from_stops(&stops);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Save as");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.save_name_buf)
                    .hint_text("preset name")
                    .desired_width(140.0),
            );
            let enter = resp.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let click = ui.button("Save preset").clicked();
            if (enter || click) && !self.save_name_buf.trim().is_empty() {
                let name = self.save_name_buf.trim().to_string();
                let stops = self.current_stops();
                let mut lib = self.library.lock().unwrap();
                lib.pending_saves.push(PendingPresetSave { name, stops });
                self.save_name_buf.clear();
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Stops").strong());

        let len = self.stops.len();
        for (i, s) in self.stops.iter_mut().enumerate() {
            ui.push_id(("stop", i), |ui| {
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut s.used, "").on_hover_text("Enable this stop").changed() {
                        changed = true;
                    }
                    ui.add_enabled_ui(s.used, |ui| {
                        let mut col = [s.r, s.g, s.b];
                        if ui.color_edit_button_rgb(&mut col).changed() {
                            s.r = col[0]; s.g = col[1]; s.b = col[2];
                            changed = true;
                        }
                        ui.label("pos");
                        if ui.add(egui::Slider::new(&mut s.position, 0.0..=1.0)
                            .step_by(0.01)
                            .show_value(true)
                        ).changed() { changed = true; }
                        ui.label("α");
                        if ui.add(egui::Slider::new(&mut s.alpha, 0.0..=1.0)
                            .step_by(0.01)
                            .show_value(true)
                        ).changed() { changed = true; }
                    });
                    if ui.add_enabled(i > 0, egui::Button::new(egui_phosphor::regular::ARROW_UP))
                        .on_hover_text("Move stop up")
                        .clicked()
                    {
                        swap = Some((i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < len, egui::Button::new(egui_phosphor::regular::ARROW_DOWN))
                        .on_hover_text("Move stop down")
                        .clicked()
                    {
                        swap = Some((i, i + 1));
                    }
                });
            });
        }

        if let Some((a, b)) = swap {
            self.stops.swap(a, b);
            changed = true;
        }

        if changed { self.push_config(); }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

fn draw_checker(painter: &egui::Painter, rect: egui::Rect) {
    let cell = 6.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    let c1 = Color32::from_gray(40);
    let c2 = Color32::from_gray(70);
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { c1 } else { c2 };
            let cell_rect = egui::Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cell_rect, 0.0, color);
        }
    }
}

/// Minimal preview-side sampler: linear interpolation in sRGB+alpha.
fn sample_preview(stops: &[(f32, Color32, f32)], t: f32) -> Color32 {
    if stops.is_empty() { return Color32::BLACK; }
    if t <= stops[0].0 { return stops[0].1; }
    if t >= stops.last().unwrap().0 { return stops.last().unwrap().1; }
    for i in 1..stops.len() {
        if t <= stops[i].0 {
            let a = &stops[i - 1];
            let b = &stops[i];
            let range = b.0 - a.0;
            let local = if range > 0.0 { (t - a.0) / range } else { 0.0 };
            let lerp = |x: u8, y: u8| (x as f32 * (1.0 - local) + y as f32 * local).round() as u8;
            return Color32::from_rgba_unmultiplied(
                lerp(a.1.r(), b.1.r()),
                lerp(a.1.g(), b.1.g()),
                lerp(a.1.b(), b.1.b()),
                lerp(a.1.a(), b.1.a()),
            );
        }
    }
    Color32::BLACK
}
