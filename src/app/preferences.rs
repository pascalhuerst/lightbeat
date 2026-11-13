use crate::LightBeatApp;
use crate::theme;
use eframe::egui;

/// MSAA sample counts a window can be asked for; 0 keeps egui's own smoothing.
const MSAA_CHOICES: [u16; 4] = [0, 2, 4, 8];

impl LightBeatApp {
    pub(crate) fn show_preferences_window(&mut self, ctx: &egui::Context) {
        if !self.show_preferences {
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new("Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let cfg = &mut self.config;
                ui.colored_label(theme::TEXT_DIM, egui::RichText::new("Editing").strong());
                changed |= ui.checkbox(&mut cfg.snap_to_grid, "Snap nodes to the grid").changed();
                changed |= ui
                    .checkbox(&mut cfg.autosave_enabled, "Autosave for crash recovery")
                    .on_hover_text("Writes a separate recovery file every 30 s while there are unsaved changes. Never over the project.")
                    .changed();

                ui.add_space(8.0);
                ui.colored_label(theme::TEXT_DIM, egui::RichText::new("Start").strong());
                changed |= ui
                    .checkbox(&mut cfg.autoload_on_open, "Open the last project at start")
                    .on_hover_text("Applies at the next start.")
                    .changed();
                changed |= ui
                    .checkbox(&mut cfg.dmx_bypass_on_startup, "Start with DMX output bypassed")
                    .on_hover_text("Wake up silent and switch Bypass off in the status bar once the right project is loaded. Applies at the next start.")
                    .changed();

                ui.add_space(8.0);
                ui.colored_label(theme::TEXT_DIM, egui::RichText::new("Performance").strong());
                ui.horizontal(|ui| {
                    ui.label("Idle frame rate");
                    let drag = egui::DragValue::new(&mut cfg.idle_repaint_hz)
                        .range(0.0..=120.0)
                        .speed(0.5)
                        .suffix(" fps")
                        .fixed_decimals(0);
                    let r = ui
                        .add(drag)
                        .on_hover_text("Frame rate while nothing is touched; full rate for 2 s after any input. 0 = always full rate. A lower value saves CPU when a show runs unattended.");
                    // Applied while dragging, written once it is let go.
                    changed |= r.drag_stopped() || r.lost_focus();
                });
                ui.horizontal(|ui| {
                    ui.label("Antialiasing (MSAA)");
                    let label = |n: u16| if n == 0 { "Off".to_string() } else { format!("{n}x") };
                    egui::ComboBox::from_id_salt("msaa_samples")
                        .selected_text(label(cfg.msaa_samples))
                        .show_ui(ui, |ui| {
                            for n in MSAA_CHOICES {
                                changed |= ui.selectable_value(&mut cfg.msaa_samples, n, label(n)).changed();
                            }
                        })
                        .response
                        .on_hover_text("Lets the graphics card smooth edges instead of egui; uses about a third less CPU. Applies at the next start.");
                });
            });
        if changed {
            self.save_settings();
        }
        self.show_preferences = open;
    }
}
