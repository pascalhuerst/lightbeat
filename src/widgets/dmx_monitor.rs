use egui::{Color32, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

const COLS: usize = 32;
const ROWS: usize = 16;
const CHANNEL_COUNT: usize = 512;

/// Colors matching the app's dark theme.
const CELL_BG: Color32 = Color32::from_rgb(30, 30, 34);
const CELL_BORDER: Color32 = Color32::from_rgb(50, 50, 56);
const LABEL_COLOR: Color32 = Color32::from_rgb(160, 160, 170);
const VALUE_COLOR: Color32 = Color32::from_rgb(200, 200, 210);

/// A DMX channel monitor widget that displays 512 channels as a grid of
/// vertical bars, similar to the Blux DMXChannelView.
///
/// Feed it a `&[u8; 512]` each frame. It does not depend on `crate::objects`
/// so it can visualize any raw DMX buffer.
pub struct DmxMonitor {
    /// Which channel the mouse is hovering over (0-based), if any.
    hovered_channel: Option<usize>,
}

impl DmxMonitor {
    pub fn new() -> Self {
        Self {
            hovered_channel: None,
        }
    }

    /// Draw the DMX monitor grid.
    ///
    /// `label`: header text (e.g. "Art-Net / Net 0 / Sub 0 / Uni 0").
    /// `channels`: the 512 DMX channel values to display.
    pub fn show(&mut self, ui: &mut Ui, label: &str, channels: &[u8; 512]) {
        // Header: show label and hovered channel detail.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label).monospace());
            ui.separator();
            if let Some(ch) = self.hovered_channel {
                ui.label(
                    egui::RichText::new(format!(
                        "Ch {:>3}: {:>3} ({:.0}%)",
                        ch + 1,
                        channels[ch],
                        channels[ch] as f32 / 255.0 * 100.0
                    ))
                    .monospace()
                    .color(VALUE_COLOR),
                );
            }
        });

        ui.separator();

        // Calculate cell size from available width.
        let avail = ui.available_size();
        let cell_w = ((avail.x - 2.0) / COLS as f32).floor().max(8.0);
        let cell_h = ((avail.y - 2.0) / ROWS as f32).floor().max(8.0);
        let cell_size = Vec2::new(cell_w, cell_h);

        let (response, painter) = ui.allocate_painter(
            Vec2::new(cell_w * COLS as f32, cell_h * ROWS as f32),
            Sense::hover(),
        );

        let origin = response.rect.left_top();
        self.hovered_channel = None;

        let mouse_pos = response.hover_pos();

        for i in 0..CHANNEL_COUNT {
            let col = i % COLS;
            let row = i / COLS;

            let top_left = origin + Vec2::new(col as f32 * cell_w, row as f32 * cell_h);
            let cell_rect = Rect::from_min_size(top_left, cell_size);

            let value = channels[i];
            let norm = value as f32 / 255.0;

            // Check hover.
            let hovered = mouse_pos
                .map(|p| cell_rect.contains(p))
                .unwrap_or(false);

            if hovered {
                self.hovered_channel = Some(i);
            }

            // Background.
            let bg = if hovered {
                Color32::from_rgb(45, 45, 52)
            } else {
                CELL_BG
            };
            painter.rect_filled(cell_rect, 1.0, bg);

            // Value bar (fills from bottom).
            if value > 0 {
                let bar_height = norm * (cell_size.y - 2.0);
                let bar_rect = Rect::from_min_max(
                    egui::pos2(
                        cell_rect.left() + 1.0,
                        cell_rect.bottom() - 1.0 - bar_height,
                    ),
                    egui::pos2(cell_rect.right() - 1.0, cell_rect.bottom() - 1.0),
                );

                // Color intensity: brighter for higher values.
                let bar_color = Color32::from_rgb(
                    (60.0 + 195.0 * norm) as u8,
                    (100.0 + 100.0 * norm) as u8,
                    255,
                );
                painter.rect_filled(bar_rect, 0.0, bar_color);
            }

            // Border.
            painter.rect_stroke(cell_rect, 1.0, Stroke::new(0.5, CELL_BORDER), StrokeKind::Inside);

            // Channel number label (only if cells are big enough).
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

            // Value label on hover.
            if hovered && value > 0 {
                let font_size = if cell_h > 30.0 { 10.0 } else { 8.0 };
                painter.text(
                    egui::pos2(cell_rect.right() - 2.0, cell_rect.top() + 1.0),
                    egui::Align2::RIGHT_TOP,
                    format!("{}", value),
                    egui::FontId::monospace(font_size),
                    VALUE_COLOR,
                );
            }
        }
    }
}
