use egui::{self, Color32, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::dmx_io::{SharedDmxState, UniverseKey};

const COLS: usize = 32;
const ROWS: usize = 16;
const CHANNEL_COUNT: usize = 512;

const CELL_BG: Color32 = Color32::from_rgb(30, 30, 34);
const CELL_BORDER: Color32 = Color32::from_rgb(50, 50, 56);
const LABEL_COLOR: Color32 = Color32::from_rgb(160, 160, 170);
const VALUE_COLOR: Color32 = Color32::from_rgb(200, 200, 210);
const OVERRIDE_COLOR: Color32 = Color32::from_rgb(240, 160, 40);

pub struct DmxMonitor {
    hovered_channel: Option<usize>,
    selected_key: Option<UniverseKey>,
}

impl DmxMonitor {
    pub fn new() -> Self {
        Self {
            hovered_channel: None,
            selected_key: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        shared: &SharedDmxState,
        interface_names: &[(u32, String)],
    ) {
        // Read available universes and current data.
        let state = shared.lock().unwrap();
        let available_keys: Vec<UniverseKey> = state.universes.keys().copied().collect();

        // Auto-select first available if nothing selected.
        if self.selected_key.is_none() || !available_keys.contains(self.selected_key.as_ref().unwrap_or(&UniverseKey { interface_id: 0, net: 0, subnet: 0, universe: 0 })) {
            self.selected_key = available_keys.first().copied();
        }

        let (channels, overrides) = if let Some(key) = &self.selected_key {
            if let Some(uni) = state.universes.get(key) {
                (uni.channels, uni.overrides.clone())
            } else {
                ([0u8; 512], crate::dmx_io::DmxOverride::new())
            }
        } else {
            ([0u8; 512], crate::dmx_io::DmxOverride::new())
        };
        drop(state);

        // Header: interface + universe selector.
        ui.horizontal(|ui| {
            // Interface + universe combo.
            let selected_label = self.selected_key.map(|k| {
                let iface_name = interface_names.iter()
                    .find(|(id, _)| *id == k.interface_id)
                    .map(|(_, n)| n.as_str())
                    .unwrap_or("???");
                format!("{} / {}", iface_name, k.label())
            }).unwrap_or_else(|| "No universes".into());

            egui::ComboBox::from_id_salt("dmx_monitor_uni")
                .selected_text(&selected_label)
                .show_ui(ui, |ui| {
                    for key in &available_keys {
                        let iface_name = interface_names.iter()
                            .find(|(id, _)| *id == key.interface_id)
                            .map(|(_, n)| n.as_str())
                            .unwrap_or("???");
                        let label = format!("{} / {}", iface_name, key.label());
                        let is_selected = self.selected_key == Some(*key);
                        if ui.selectable_label(is_selected, &label).clicked() {
                            self.selected_key = Some(*key);
                        }
                    }
                    if available_keys.is_empty() {
                        ui.label("No active universes");
                    }
                });

            ui.separator();

            if ui.small_button("Clear Overrides").clicked() {
                if let Some(key) = &self.selected_key {
                    let mut state = shared.lock().unwrap();
                    if let Some(uni) = state.universes.get_mut(key) {
                        uni.overrides.clear_all();
                    }
                }
            }

            ui.separator();

            if let Some(ch) = self.hovered_channel {
                let ovr = if overrides.active[ch] { " [OVR]" } else { "" };
                ui.label(
                    egui::RichText::new(format!(
                        "Ch {:>3}: {:>3} ({:.0}%){}",
                        ch + 1,
                        channels[ch],
                        channels[ch] as f32 / 255.0 * 100.0,
                        ovr,
                    ))
                    .monospace()
                    .color(if overrides.active[ch] { OVERRIDE_COLOR } else { VALUE_COLOR }),
                );
            }
        });

        ui.separator();

        // Grid.
        let avail = ui.available_size();
        let cell_w = ((avail.x - 2.0) / COLS as f32).floor().max(8.0);
        let cell_h = ((avail.y - 2.0) / ROWS as f32).floor().max(8.0);
        let cell_size = Vec2::new(cell_w, cell_h);

        let (response, painter) = ui.allocate_painter(
            Vec2::new(cell_w * COLS as f32, cell_h * ROWS as f32),
            Sense::click_and_drag(),
        );

        let origin = response.rect.left_top();
        self.hovered_channel = None;

        let mouse_pos = response.hover_pos();
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

        // Ctrl+click/drag for overrides.
        if ctrl && (response.dragged() || response.clicked()) {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(key) = &self.selected_key {
                    let col = ((pos.x - origin.x) / cell_w).floor() as usize;
                    let row = ((pos.y - origin.y) / cell_h).floor() as usize;
                    if col < COLS && row < ROWS {
                        let ch = row * COLS + col;
                        if ch < CHANNEL_COUNT {
                            let cell_top = origin.y + row as f32 * cell_h;
                            let norm = 1.0 - ((pos.y - cell_top) / cell_h).clamp(0.0, 1.0);
                            let value = (norm * 255.0).round() as u8;
                            let mut state = shared.lock().unwrap();
                            if let Some(uni) = state.universes.get_mut(key) {
                                uni.overrides.set(ch, value);
                            }
                        }
                    }
                }
            }
        }

        for i in 0..CHANNEL_COUNT {
            let col = i % COLS;
            let row = i / COLS;

            let top_left = origin + Vec2::new(col as f32 * cell_w, row as f32 * cell_h);
            let cell_rect = Rect::from_min_size(top_left, cell_size);

            let value = channels[i];
            let norm = value as f32 / 255.0;
            let is_override = overrides.active[i];

            let hovered = mouse_pos.map(|p| cell_rect.contains(p)).unwrap_or(false);
            if hovered { self.hovered_channel = Some(i); }

            let bg = if hovered { Color32::from_rgb(45, 45, 52) } else { CELL_BG };
            painter.rect_filled(cell_rect, 1.0, bg);

            if value > 0 {
                let bar_height = norm * (cell_size.y - 2.0);
                let bar_rect = Rect::from_min_max(
                    egui::pos2(cell_rect.left() + 1.0, cell_rect.bottom() - 1.0 - bar_height),
                    egui::pos2(cell_rect.right() - 1.0, cell_rect.bottom() - 1.0),
                );
                let bar_color = if is_override {
                    Color32::from_rgb(
                        (200.0 + 55.0 * norm) as u8,
                        (120.0 + 80.0 * norm) as u8,
                        (20.0 + 30.0 * norm) as u8,
                    )
                } else {
                    Color32::from_rgb(
                        (60.0 + 195.0 * norm) as u8,
                        (100.0 + 100.0 * norm) as u8,
                        255,
                    )
                };
                painter.rect_filled(bar_rect, 0.0, bar_color);
            }

            let border_color = if is_override { OVERRIDE_COLOR.linear_multiply(0.5) } else { CELL_BORDER };
            painter.rect_stroke(cell_rect, 1.0, Stroke::new(0.5, border_color), StrokeKind::Inside);

            if cell_w >= 16.0 && cell_h >= 20.0 {
                let font_size = if cell_h > 30.0 { 9.0 } else { 7.0 };
                painter.text(
                    egui::pos2(cell_rect.left() + 2.0, cell_rect.bottom() - font_size - 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", i + 1),
                    egui::FontId::monospace(font_size),
                    LABEL_COLOR.gamma_multiply(0.5),
                );
            }

            if hovered && value > 0 {
                let font_size = if cell_h > 30.0 { 10.0 } else { 8.0 };
                painter.text(
                    egui::pos2(cell_rect.right() - 2.0, cell_rect.top() + 1.0),
                    egui::Align2::RIGHT_TOP,
                    format!("{}", value),
                    egui::FontId::monospace(font_size),
                    if is_override { OVERRIDE_COLOR } else { VALUE_COLOR },
                );
            }
        }
    }
}
