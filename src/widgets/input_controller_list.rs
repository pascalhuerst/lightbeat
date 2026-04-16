use egui::{self, Color32, Ui};

use crate::input_controller::{
    ConnectionStatus, InputBindingMode, InputControllerKind, InputControllerManager,
};

pub fn show(ui: &mut Ui, mgr: &mut InputControllerManager) {
    ui.heading("Input Controllers");
    ui.separator();

    // Refresh available MIDI ports each frame (cheap on macOS/Linux/Windows;
    // midir just enumerates the system).
    let available_ports = InputControllerManager::available_midi_ports();

    let mut consume_learn_for: Vec<u32> = Vec::new();
    let mut remove_controller: Option<u32> = None;
    // (controller_id, input_id)
    let mut remove_input: Vec<(u32, u32)> = Vec::new();
    // (controller_id, input_id, new_name)
    let mut rename_input: Vec<(u32, u32, String)> = Vec::new();
    // (controller_id, input_id, new_mode)
    let mut set_mode: Vec<(u32, u32, InputBindingMode)> = Vec::new();
    // (controller_id, new_name)
    let mut rename_controller: Vec<(u32, String)> = Vec::new();
    // (controller_id, port_name)
    let mut set_port: Vec<(u32, String)> = Vec::new();
    // (controller_id, learning)
    let mut set_learning: Vec<(u32, bool)> = Vec::new();

    {
        let state = mgr.shared.lock().unwrap();
        if state.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No input controllers. Add one below.");
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for c in state.iter() {
                ui.push_id(c.id, |ui| {
                    let header = format!("{} ({})", c.name, c.kind.label());
                    egui::CollapsingHeader::new(egui::RichText::new(header).strong())
                        .id_salt(c.id)
                        .default_open(true)
                        .show(ui, |ui| {
                            // Name
                            ui.horizontal(|ui| {
                                ui.label("Name:");
                                let mut name = c.name.clone();
                                if ui.text_edit_singleline(&mut name).changed() {
                                    rename_controller.push((c.id, name));
                                }
                            });

                            // Hardware port + status
                            match &c.kind {
                                InputControllerKind::Midi { hw_port_name } => {
                                    ui.horizontal(|ui| {
                                        ui.label("MIDI Port:");
                                        let label = if hw_port_name.is_empty() {
                                            "(none)".to_string()
                                        } else {
                                            hw_port_name.clone()
                                        };
                                        egui::ComboBox::from_id_salt(("port", c.id))
                                            .selected_text(label)
                                            .show_ui(ui, |ui| {
                                                if ui.selectable_label(hw_port_name.is_empty(), "(none)").clicked() {
                                                    set_port.push((c.id, String::new()));
                                                }
                                                for p in &available_ports {
                                                    if ui.selectable_label(p == hw_port_name, p).clicked() {
                                                        set_port.push((c.id, p.clone()));
                                                    }
                                                }
                                            });
                                        let (status_text, status_color) = match c.status {
                                            ConnectionStatus::Connected => ("Connected", Color32::from_rgb(80, 200, 80)),
                                            ConnectionStatus::Waiting => ("Waiting", Color32::from_rgb(220, 180, 60)),
                                            ConnectionStatus::Disconnected => ("No mapping", Color32::from_gray(140)),
                                        };
                                        ui.colored_label(status_color, status_text);
                                    });
                                }
                            }

                            // Learn mode toggle
                            ui.horizontal(|ui| {
                                let mut learning = c.learning;
                                let label = if learning { "Stop Learning" } else { "Learn" };
                                if ui.toggle_value(&mut learning, label).changed() {
                                    set_learning.push((c.id, learning));
                                }
                                if c.learning {
                                    ui.colored_label(
                                        Color32::from_rgb(220, 180, 60),
                                        "Move a fader / press a button on the device...",
                                    );
                                    if !c.learn_buffer.is_empty() {
                                        consume_learn_for.push(c.id);
                                    }
                                }
                            });

                            ui.separator();

                            // Inputs list
                            ui.label(egui::RichText::new("Inputs").strong());
                            if c.inputs.is_empty() {
                                ui.colored_label(Color32::from_gray(120), "No inputs yet.");
                            }
                            for (idx, input) in c.inputs.iter().enumerate() {
                                ui.push_id(("input", input.id), |ui| {
                                    ui.horizontal(|ui| {
                                        let mut name = input.name.clone();
                                        if ui.add(
                                            egui::TextEdit::singleline(&mut name).desired_width(100.0),
                                        ).changed() {
                                            rename_input.push((c.id, input.id, name));
                                        }
                                        ui.colored_label(
                                            Color32::from_gray(150),
                                            input.source.label(),
                                        );
                                        // Live value preview.
                                        let v = c.values.get(idx).copied().unwrap_or(0.0);
                                        ui.colored_label(
                                            Color32::from_gray(180),
                                            format!("{:.2}", v),
                                        );
                                        if ui.small_button(egui_phosphor::regular::X).clicked() {
                                            remove_input.push((c.id, input.id));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("    Mode:");
                                        let binary = input.source.is_binary();
                                        let mut mode = input.mode;
                                        let prev = mode;
                                        if ui.radio_value(&mut mode, InputBindingMode::Value, "Value").clicked() {}
                                        if binary {
                                            if ui.radio_value(&mut mode, InputBindingMode::TriggerOnPress, "Press").clicked() {}
                                            if ui.radio_value(&mut mode, InputBindingMode::TriggerOnRelease, "Release").clicked() {}
                                        }
                                        if mode != prev {
                                            set_mode.push((c.id, input.id, mode));
                                        }
                                    });
                                });
                            }

                            ui.add_space(4.0);
                            if ui.small_button("Delete controller").clicked() {
                                remove_controller = Some(c.id);
                            }
                        });
                });
            }
        });
    }

    ui.separator();
    if ui.button("+ Add MIDI Controller").clicked() {
        mgr.add_controller("Controller".to_string());
    }

    // Apply queued mutations (lock was released above).
    for (id, name) in rename_controller { mgr.rename(id, name); }
    for (id, port) in set_port { mgr.set_hw_port(id, port); }
    for (id, learning) in set_learning { mgr.set_learning(id, learning); }
    for cid in consume_learn_for { let _ = mgr.consume_learn(cid); }
    for (cid, iid, name) in rename_input { mgr.rename_input(cid, iid, name); }
    for (cid, iid, mode) in set_mode { mgr.set_input_mode(cid, iid, mode); }
    for (cid, iid) in remove_input { mgr.remove_input(cid, iid); }
    if let Some(id) = remove_controller { mgr.remove_controller(id); }
}
