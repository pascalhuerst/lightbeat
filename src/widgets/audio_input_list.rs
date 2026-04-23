use egui::{self, Color32, Ui};

use crate::audio::analyzers::AnalyzerKind;
use crate::audio::backend::AudioBackendKind;
use crate::audio::manager::{AudioInputManager, ConnectionStatus};

pub fn show(ui: &mut Ui, mgr: &mut AudioInputManager) {
    ui.horizontal(|ui| {
        ui.heading("Audio Inputs");
        if ui.small_button("Refresh devices").clicked() {
            mgr.force_rescan();
        }
    });
    ui.separator();

    // Snapshot device lists for each backend before the shared-state lock so
    // the UI closure doesn't borrow `mgr` twice.
    let mgr_devices: std::collections::HashMap<AudioBackendKind, Vec<String>> =
        AudioBackendKind::ALL.iter()
            .map(|k| (*k, mgr.cached_devices_for(*k).to_vec()))
            .collect();

    let mut remove_input: Option<u32> = None;
    let mut rename_input: Vec<(u32, String)> = Vec::new();
    let mut set_enabled: Vec<(u32, bool)> = Vec::new();
    let mut set_backend: Vec<(u32, AudioBackendKind)> = Vec::new();
    let mut set_device: Vec<(u32, String)> = Vec::new();
    let mut set_rate: Vec<(u32, Option<u32>)> = Vec::new();
    let mut set_buffer: Vec<(u32, Option<u32>)> = Vec::new();
    let mut add_analyzer: Vec<(u32, AnalyzerKind)> = Vec::new();
    let mut remove_analyzer: Vec<(u32, usize)> = Vec::new();

    {
        let state = mgr.shared.lock().unwrap();
        if state.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No audio inputs. Add one below.");
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for c in state.iter() {
                ui.push_id(c.id, |ui| {
                    let header = format!("{} ({})", c.name, c.device_name.as_str());
                    egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                        .id_salt(c.id)
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Name:");
                                let mut name = c.name.clone();
                                if ui.text_edit_singleline(&mut name).changed() {
                                    rename_input.push((c.id, name));
                                }
                                let mut enabled = c.enabled;
                                if ui.checkbox(&mut enabled, "Enabled")
                                    .on_hover_text(
                                        "When off, this input releases its device and stops all \
                                         analyzers. Turn on to reconnect.",
                                    )
                                    .changed()
                                {
                                    set_enabled.push((c.id, enabled));
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label("Backend:");
                                egui::ComboBox::from_id_salt(("backend", c.id))
                                    .selected_text(c.backend.label())
                                    .show_ui(ui, |ui| {
                                        for k in AudioBackendKind::ALL {
                                            if ui.selectable_label(c.backend == *k, k.label()).clicked() {
                                                set_backend.push((c.id, *k));
                                            }
                                        }
                                    });
                            });
                            let available_devices = mgr_devices.get(&c.backend)
                                .cloned()
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.label("Device:");
                                let label = if c.device_name.is_empty() { "(none)".to_string() } else { c.device_name.clone() };
                                egui::ComboBox::from_id_salt(("dev", c.id))
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(c.device_name.is_empty(), "(none)").clicked() {
                                            set_device.push((c.id, String::new()));
                                        }
                                        for d in &available_devices {
                                            if ui.selectable_label(d == &c.device_name, d).clicked() {
                                                set_device.push((c.id, d.clone()));
                                            }
                                        }
                                    });
                                let (status_text, status_color) = match c.status {
                                    ConnectionStatus::Connected => ("Connected", Color32::from_rgb(80, 200, 80)),
                                    ConnectionStatus::Waiting => ("Waiting", Color32::from_rgb(220, 180, 60)),
                                    ConnectionStatus::Disconnected => ("No mapping", Color32::from_gray(140)),
                                    ConnectionStatus::Disabled => ("Disabled", Color32::from_gray(120)),
                                };
                                ui.colored_label(status_color, status_text);
                            });

                            // Sample rate dropdown — fixed common-rate list;
                            // cpal negotiates the closest supported rate at
                            // stream-open time, which is reflected in
                            // `actual_sample_rate` below.
                            let rates: &[u32] = AudioInputManager::COMMON_SAMPLE_RATES;
                            ui.horizontal(|ui| {
                                ui.label("Sample rate:");
                                let cur = c.sample_rate;
                                let label = match cur {
                                    Some(r) => format!("{} Hz", r),
                                    None => "Default".to_string(),
                                };
                                egui::ComboBox::from_id_salt(("rate", c.id))
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(cur.is_none(), "Default").clicked() {
                                            set_rate.push((c.id, None));
                                        }
                                        for r in rates {
                                            if ui.selectable_label(cur == Some(*r), format!("{} Hz", r)).clicked() {
                                                set_rate.push((c.id, Some(*r)));
                                            }
                                        }
                                    });
                                if c.actual_sample_rate != 0 {
                                    ui.colored_label(
                                        Color32::from_gray(140),
                                        format!("(actual: {} Hz)", c.actual_sample_rate),
                                    );
                                }
                            });

                            // Buffer size (frames). Common power-of-two choices.
                            ui.horizontal(|ui| {
                                ui.label("Buffer size:");
                                let cur = c.buffer_size_frames;
                                let label = match cur {
                                    Some(b) => format!("{} frames", b),
                                    None => "Default".to_string(),
                                };
                                egui::ComboBox::from_id_salt(("buf", c.id))
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(cur.is_none(), "Default").clicked() {
                                            set_buffer.push((c.id, None));
                                        }
                                        for b in [128, 256, 512, 1024, 2048, 4096] {
                                            if ui.selectable_label(cur == Some(b), format!("{} frames", b)).clicked() {
                                                set_buffer.push((c.id, Some(b)));
                                            }
                                        }
                                    });
                                if c.actual_buffer_frames != 0 {
                                    ui.colored_label(
                                        Color32::from_gray(140),
                                        format!("(actual: {} frames)", c.actual_buffer_frames),
                                    );
                                }
                            });

                            ui.separator();
                            ui.label(egui::RichText::new("Analyzers").strong());

                            if c.analyzer_kinds.is_empty() {
                                ui.colored_label(Color32::from_gray(120), "No analyzers.");
                            }
                            for (idx, kind) in c.analyzer_kinds.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}. {}", idx + 1, kind.label()));
                                    if ui.small_button(egui_phosphor::regular::X).clicked() {
                                        remove_analyzer.push((c.id, idx));
                                    }
                                });
                            }
                            ui.horizontal(|ui| {
                                let mut add_kind: Option<AnalyzerKind> = None;
                                egui::ComboBox::from_id_salt(("add_an", c.id))
                                    .selected_text("+ Add analyzer")
                                    .show_ui(ui, |ui| {
                                        for k in AnalyzerKind::ALL {
                                            if ui.selectable_label(false, k.label()).clicked() {
                                                add_kind = Some(k);
                                            }
                                        }
                                    });
                                if let Some(k) = add_kind {
                                    add_analyzer.push((c.id, k));
                                }
                            });

                            ui.add_space(4.0);
                            if ui.small_button("Delete input").clicked() {
                                remove_input = Some(c.id);
                            }
                        });
                });
            }
        });
    }

    ui.separator();
    if ui.button("+ Add Audio Input").clicked() {
        mgr.add_input("Audio Input".to_string());
    }

    // Apply queued mutations after the lock is released.
    for (id, name) in rename_input { mgr.rename(id, name); }
    for (id, on) in set_enabled { mgr.set_enabled(id, on); }
    for (id, b) in set_backend { mgr.set_backend(id, b); }
    for (id, dev) in set_device { mgr.set_device(id, dev); }
    for (id, sr) in set_rate { mgr.set_sample_rate(id, sr); }
    for (id, bs) in set_buffer { mgr.set_buffer_size(id, bs); }
    for (id, kind) in add_analyzer { mgr.add_analyzer(id, kind); }
    for (id, idx) in remove_analyzer { mgr.remove_analyzer(id, idx); }
    if let Some(id) = remove_input { mgr.remove_input(id); }
}
