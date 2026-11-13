use egui::{self, Ui};

use crate::audio::analyzers::AnalyzerKind;
use crate::audio::backend::AudioBackendKind;
use crate::audio::manager::{AudioInputManager, ConnectionStatus};
use crate::theme;
use crate::widgets::setup_ui::{self, ItemHeader, Status};

fn status_of(status: ConnectionStatus) -> Status {
    match status {
        ConnectionStatus::Connected => Status::Connected,
        ConnectionStatus::Waiting => Status::Waiting,
        ConnectionStatus::Disconnected => Status::NoMapping,
        ConnectionStatus::Disabled => Status::Disabled,
    }
}

/// A new input with the same device, rates and analyzers. The copy shares the
/// hardware with the original, so it only opens when the device allows it.
fn duplicate_input(mgr: &mut AudioInputManager, id: u32) {
    let Some(src) = mgr.export().into_iter().find(|c| c.id == id) else { return };
    let new_id = mgr.add_input(setup_ui::copy_name(&src.name));
    mgr.set_backend(new_id, src.backend);
    mgr.set_device(new_id, src.device_name);
    mgr.set_sample_rate(new_id, src.sample_rate);
    mgr.set_buffer_size(new_id, src.buffer_size_frames);
    for a in &src.analyzers {
        mgr.add_analyzer(new_id, a.kind);
    }
    if !src.enabled {
        mgr.set_enabled(new_id, false);
    }
}

pub fn show(ui: &mut Ui, mgr: &mut AudioInputManager) {
    ui.horizontal(|ui| {
        if ui
            .button(format!("{} Rescan", egui_phosphor::regular::ARROWS_CLOCKWISE))
            .on_hover_text("Look for audio devices again")
            .clicked()
        {
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
    let mut duplicate: Option<u32> = None;
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
            setup_ui::empty_state(ui, "No audio inputs yet. Add one below.");
        }

        let open = setup_ui::open_by_default(state.len());
        for c in state.iter() {
            let title = format!("{} ({})", c.name, c.device_name.as_str());
            let mut header = ItemHeader::new(&title, "Delete audio input");
            header.duplicate = Some("Duplicate audio input");
            header.enabled = Some(c.enabled);
            header.status = Some(status_of(c.status));
            let actions = setup_ui::item_header(ui, c.id, open, header, |ui| {
                let mut name = c.name.clone();
                if setup_ui::name_field(ui, &mut name) {
                    rename_input.push((c.id, name));
                }

                ui.horizontal(|ui| {
                    ui.label("Backend");
                    egui::ComboBox::from_id_salt(("backend", c.id))
                        .selected_text(c.backend.label())
                        .show_ui(ui, |ui| {
                            for k in &AudioBackendKind::available() {
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
                    ui.label("Device");
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
                });

                // Sample rate dropdown: fixed common-rate list; cpal
                // negotiates the closest supported rate at stream-open time,
                // which is reflected in `actual_sample_rate` below.
                let rates: &[u32] = AudioInputManager::COMMON_SAMPLE_RATES;
                ui.horizontal(|ui| {
                    ui.label("Sample rate");
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
                            theme::TEXT_DIM,
                            format!("(actual: {} Hz)", c.actual_sample_rate),
                        );
                    }
                });

                // Buffer size (frames). Common power-of-two choices.
                ui.horizontal(|ui| {
                    ui.label("Buffer size");
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
                            theme::TEXT_DIM,
                            format!("(actual: {} frames)", c.actual_buffer_frames),
                        );
                    }
                });

                ui.separator();
                setup_ui::section(ui, "Analyzers");

                if c.analyzer_kinds.is_empty() {
                    setup_ui::empty_state(ui, "No analyzers yet. Add one below.");
                }
                for (idx, kind) in c.analyzer_kinds.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}. {}", idx + 1, kind.label()));
                        if setup_ui::delete_button(ui, "Delete analyzer") {
                            remove_analyzer.push((c.id, idx));
                        }
                    });
                }
                let kinds = AnalyzerKind::ALL.iter().map(|k| (*k, k.label().to_string())).collect();
                if let Some(k) = setup_ui::add_menu(ui, ("add_an", c.id), "Add analyzer...", kinds) {
                    add_analyzer.push((c.id, k));
                }
                ui.add_space(4.0);
            });
            if let Some(on) = actions.enabled {
                set_enabled.push((c.id, on));
            }
            if actions.delete {
                remove_input = Some(c.id);
            }
            if actions.duplicate {
                duplicate = Some(c.id);
            }
        }
    }

    ui.separator();
    if setup_ui::add_button(ui, "Add audio input") {
        mgr.add_input("Audio input".to_string());
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
    if let Some(id) = duplicate { duplicate_input(mgr, id); }
    if let Some(id) = remove_input { mgr.remove_input(id); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_connection_state_has_a_word() {
        assert_eq!(status_of(ConnectionStatus::Connected), Status::Connected);
        assert_eq!(status_of(ConnectionStatus::Waiting), Status::Waiting);
        assert_eq!(status_of(ConnectionStatus::Disconnected), Status::NoMapping);
        assert_eq!(status_of(ConnectionStatus::Disabled), Status::Disabled);
    }
}
