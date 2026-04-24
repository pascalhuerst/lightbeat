use egui::{self, Color32, Ui};
use egui_extras::{Column, TableBuilder};

use crate::input_controller::{
    ConnectionStatus, InputBindingMode, InputControllerKind, InputControllerManager, InputSource,
};
use crate::input_controller::midi::MidiSource;

pub fn show(ui: &mut Ui, mgr: &mut InputControllerManager) {
    ui.heading("Input Controllers");
    ui.separator();

    // Refresh available MIDI ports each frame (cheap on macOS/Linux/Windows;
    // midir just enumerates the system).
    let available_ports = InputControllerManager::available_midi_ports();
    let available_output_ports = InputControllerManager::available_midi_output_ports();

    let mut consume_learn_for: Vec<u32> = Vec::new();
    let mut remove_controller: Option<u32> = None;
    let mut remove_input: Vec<(u32, u32)> = Vec::new();
    let mut rename_input: Vec<(u32, u32, String)> = Vec::new();
    let mut set_mode: Vec<(u32, u32, InputBindingMode)> = Vec::new();
    // (controller_id, input_id, disable_feedback)
    let mut set_feedback_disabled: Vec<(u32, u32, bool)> = Vec::new();
    let mut rename_controller: Vec<(u32, String)> = Vec::new();
    let mut set_port: Vec<(u32, String)> = Vec::new();
    let mut set_output_port: Vec<(u32, String)> = Vec::new();
    let mut set_learning: Vec<(u32, bool)> = Vec::new();
    // Debug: (controller_id, input_index, value). Applied directly to
    // out_values (and values when loopback is on) after the read lock is released.
    let mut debug_set_out: Vec<(u32, usize, f32)> = Vec::new();
    // (controller_id, debug_open)
    let mut set_debug_open: Vec<(u32, bool)> = Vec::new();
    // (controller_id, debug_feedback_override, debug_loopback, debug_highlight_on_touch)
    let mut set_debug_flags: Vec<(u32, Option<bool>, Option<bool>, Option<bool>)> = Vec::new();
    // (controller_id) — clear log
    let mut clear_midi_log: Vec<u32> = Vec::new();
    // (controller_id, input_id or None to cancel) — arm per-row relearn
    let mut set_relearn: Vec<(u32, Option<u32>)> = Vec::new();
    // controller_id — reset inputs to factory defaults
    let mut reset_factory: Vec<u32> = Vec::new();

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

                            // Hardware input port — only shown for MIDI-based
                            // kinds. X1 is USB HID (auto-discovered by
                            // VID/PID) so no port picker applies; surface
                            // just the connection status on its own row.
                            let is_midi_kind = !matches!(c.kind, InputControllerKind::X1);
                            if is_midi_kind {
                                let input_port = c.kind.input_port().to_string();
                                ui.horizontal(|ui| {
                                    ui.label("MIDI In:");
                                    let label = if input_port.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        input_port.clone()
                                    };
                                    egui::ComboBox::from_id_salt(("port", c.id))
                                        .selected_text(label)
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_label(input_port.is_empty(), "(none)").clicked() {
                                                set_port.push((c.id, String::new()));
                                            }
                                            for p in &available_ports {
                                                if ui.selectable_label(p == &input_port, p).clicked() {
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
                            } else {
                                // X1: just show the connection badge.
                                ui.horizontal(|ui| {
                                    ui.label("USB:");
                                    let (status_text, status_color) = match c.status {
                                        ConnectionStatus::Connected => ("Connected", Color32::from_rgb(80, 200, 80)),
                                        ConnectionStatus::Waiting => ("Waiting for device", Color32::from_rgb(220, 180, 60)),
                                        ConnectionStatus::Disconnected => ("Disconnected", Color32::from_gray(140)),
                                    };
                                    ui.colored_label(status_color, status_text);
                                });
                            }

                            // Feedback-capable MIDI kinds (BCF2000 / Push 1)
                            // also pick an output port. X1's feedback goes
                            // over the same USB endpoint, so no port picker.
                            if c.kind.has_feedback() && is_midi_kind {
                                let output_port = c.kind.output_port().to_string();
                                ui.horizontal(|ui| {
                                    ui.label("MIDI Out:");
                                    let label = if output_port.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        output_port.clone()
                                    };
                                    egui::ComboBox::from_id_salt(("out_port", c.id))
                                        .selected_text(label)
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_label(output_port.is_empty(), "(none)").clicked() {
                                                set_output_port.push((c.id, String::new()));
                                            }
                                            for p in &available_output_ports {
                                                if ui.selectable_label(p == &output_port, p).clicked() {
                                                    set_output_port.push((c.id, p.clone()));
                                                }
                                            }
                                        });
                                });
                            }

                            // For factory-preset kinds (BCF2000 / Push 1) the
                            // shipping CC numbers are just a starting point;
                            // the user can re-learn per row, add / delete,
                            // or reset to factory.
                            let is_factory_kind = matches!(
                                c.kind,
                                InputControllerKind::Bcf2000 { .. } | InputControllerKind::Push1 { .. },
                            );
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
                                if is_factory_kind
                                    && ui.button("Reset to factory preset")
                                        .on_hover_text("Replace all inputs with the shipped factory defaults.")
                                        .clicked()
                                    {
                                        reset_factory.push(c.id);
                                    }
                            });

                            ui.separator();

                            // Inputs table — one row per input, columns are
                            // the editable/readable properties. Wrapped in a
                            // ScrollArea so 120+ Push-sized layouts stay
                            // manageable.
                            ui.label(egui::RichText::new("Inputs").strong());
                            if c.inputs.is_empty() {
                                ui.colored_label(Color32::from_gray(120), "No inputs yet.");
                            } else {
                                show_inputs_table(
                                    ui,
                                    c,
                                    is_factory_kind,
                                    &mut rename_input,
                                    &mut set_mode,
                                    &mut set_feedback_disabled,
                                    &mut remove_input,
                                    &mut set_relearn,
                                );
                            }

                            ui.add_space(4.0);
                            // Debug / test panel.
                            let mut dbg_open = c.debug_open;
                            if ui.toggle_value(&mut dbg_open, "Debug / Test").changed() {
                                set_debug_open.push((c.id, dbg_open));
                            }
                            if c.debug_open {
                                show_debug_panel(
                                    ui,
                                    c,
                                    &mut debug_set_out,
                                    &mut set_debug_flags,
                                    &mut clear_midi_log,
                                );
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
    ui.horizontal(|ui| {
        if ui.button("+ Add MIDI Controller").clicked() {
            mgr.add_controller("Controller".to_string());
        }
        if ui.button("+ Add BCF2000").clicked() {
            mgr.add_bcf2000("BCF2000".to_string());
        }
        if ui.button("+ Add Push 1").clicked() {
            mgr.add_push1("Push 1".to_string());
        }
        if ui.button("+ Add X1").clicked() {
            mgr.add_x1("X1".to_string());
        }
        if ui.button("+ Add Launchpad S").clicked() {
            mgr.add_launchpad("Launchpad S".to_string());
        }
    });

    // Apply queued mutations (lock was released above).
    for (id, name) in rename_controller { mgr.rename(id, name); }
    for (id, port) in set_port { mgr.set_hw_port(id, port); }
    for (id, port) in set_output_port { mgr.set_hw_output_port(id, port); }
    for (id, learning) in set_learning { mgr.set_learning(id, learning); }
    for cid in consume_learn_for { let _ = mgr.consume_learn(cid); }
    for (cid, iid, name) in rename_input { mgr.rename_input(cid, iid, name); }
    for (cid, iid, mode) in set_mode { mgr.set_input_mode(cid, iid, mode); }
    for (cid, iid, d) in set_feedback_disabled { mgr.set_input_feedback_disabled(cid, iid, d); }
    for (cid, iid) in remove_input { mgr.remove_input(cid, iid); }
    for (cid, iid) in set_relearn { mgr.set_relearn(cid, iid); }
    for cid in reset_factory { mgr.reset_factory_layout(cid); }
    // Debug panel writes: apply by taking the shared lock once.
    if !debug_set_out.is_empty() || !set_debug_open.is_empty()
        || !set_debug_flags.is_empty() || !clear_midi_log.is_empty()
    {
        let mut state = mgr.shared.lock().unwrap();
        for (cid, open) in set_debug_open {
            if let Some(c) = state.iter_mut().find(|c| c.id == cid) {
                c.debug_open = open;
            }
        }
        for (cid, override_flag, loopback, highlight) in set_debug_flags {
            if let Some(c) = state.iter_mut().find(|c| c.id == cid) {
                if let Some(v) = override_flag { c.debug_feedback_override = v; }
                if let Some(v) = loopback { c.debug_loopback = v; }
                if let Some(v) = highlight {
                    c.debug_highlight_on_touch = v;
                    if !v {
                        c.last_match_idx = None;
                        c.last_match_instant = None;
                    }
                }
            }
        }
        for cid in clear_midi_log {
            if let Some(c) = state.iter_mut().find(|c| c.id == cid) {
                c.activity_log.clear();
            }
        }
        for (cid, idx, v) in debug_set_out {
            if let Some(c) = state.iter_mut().find(|c| c.id == cid) {
                if let Some(slot) = c.out_values.get_mut(idx) { *slot = v; }
                if c.debug_loopback
                    && let Some(slot) = c.values.get_mut(idx) { *slot = v; }
            }
        }
    }
    if let Some(id) = remove_controller { mgr.remove_controller(id); }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}

fn show_inputs_table(
    ui: &mut Ui,
    c: &crate::input_controller::ControllerRuntime,
    _is_factory_kind: bool,
    rename_input: &mut Vec<(u32, u32, String)>,
    set_mode: &mut Vec<(u32, u32, InputBindingMode)>,
    set_feedback_disabled: &mut Vec<(u32, u32, bool)>,
    remove_input: &mut Vec<(u32, u32)>,
    set_relearn: &mut Vec<(u32, Option<u32>)>,
) {
    let has_feedback = c.kind.has_feedback();
    let now = std::time::Instant::now();
    /// Time the highlight stays visible after a match, in seconds. After
    /// this many seconds the row fades back to normal colors. Scroll-to only
    /// fires during the first `SCROLL_WINDOW` seconds so later touches of
    /// the same control don't fight the user's own scrolling.
    const HIGHLIGHT_SECS: f32 = 1.5;
    const SCROLL_WINDOW: f32 = 0.1;

    let highlight_row_age = if c.debug_highlight_on_touch {
        c.last_match_instant
            .map(|t| now.duration_since(t).as_secs_f32())
    } else {
        None
    };
    let highlight_idx = c.last_match_idx;

    // Keep repainting while the highlight is fading so the color animates
    // even when the user isn't moving the mouse.
    if matches!(highlight_row_age, Some(age) if age < HIGHLIGHT_SECS) {
        ui.ctx().request_repaint();
    }

    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .vscroll(true)
        .max_scroll_height(260.0)
        .column(Column::remainder().at_least(120.0).clip(true)) // Name
        .column(Column::remainder().at_least(120.0).clip(true)) // Source
        .column(Column::initial(140.0).at_least(80.0))          // Mode
        .column(Column::exact(60.0));                            // Value
    let table = if has_feedback {
        table.column(Column::exact(36.0))                        // FB (feedback toggle)
    } else {
        table
    };
    let mut table = table
        .column(Column::exact(44.0))                             // Learn
        .column(Column::exact(24.0));                            // delete

    if let (Some(idx), Some(age)) = (highlight_idx, highlight_row_age)
        && age < SCROLL_WINDOW {
            table = table.scroll_to_row(idx + 1, Some(egui::Align::Center));
        }

    let armed_relearn = c.relearn_input_id;

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Source"); });
            header.col(|ui| { ui.strong("Mode"); });
            header.col(|ui| { ui.strong("Value"); });
            if has_feedback {
                header.col(|ui| {
                    ui.strong("FB").on_hover_text(
                        "Feedback enabled: when on, this mapping receives values from the graph and emits MIDI back to the device, and a feedback input port appears on the Input Controller node."
                    );
                });
            }
            header.col(|ui| { ui.strong("Learn"); });
            header.col(|_ui| {});
        })
        .body(|mut body| {
            for (idx, input) in c.inputs.iter().enumerate() {
                let hl = match (highlight_idx, highlight_row_age) {
                    (Some(m), Some(age)) if m == idx && age < HIGHLIGHT_SECS => {
                        1.0 - (age / HIGHLIGHT_SECS)
                    }
                    _ => 0.0,
                };
                let hl_text_color = if hl > 0.0 {
                    Some(lerp_color(
                        Color32::from_gray(220),
                        Color32::from_rgb(255, 220, 90),
                        hl,
                    ))
                } else {
                    None
                };
                let this_armed = armed_relearn == Some(input.id);

                body.row(22.0, |mut row| {
                    // Name — always editable so the user can tidy up factory
                    // labels to match their workflow.
                    row.col(|ui| {
                        let mut name = input.name.clone();
                        let mut edit = egui::TextEdit::singleline(&mut name)
                            .id_salt(("name", input.id));
                        if let Some(cc) = hl_text_color {
                            edit = edit.text_color(cc);
                        }
                        if ui.add_sized([ui.available_width(), 20.0], edit).changed() {
                            rename_input.push((c.id, input.id, name));
                        }
                    });
                    // Source label. Highlighted text when the row is armed
                    // for relearn so it's obvious what will change.
                    row.col(|ui| {
                        let color = if this_armed {
                            Color32::from_rgb(220, 180, 60)
                        } else {
                            hl_text_color.unwrap_or(Color32::from_gray(160))
                        };
                        ui.colored_label(
                            color,
                            egui::RichText::new(if this_armed {
                                "waiting for MIDI...".to_string()
                            } else {
                                input.source.label()
                            })
                                .monospace()
                                .size(11.0),
                        );
                    });
                    // Mode combo — also editable for factory kinds now.
                    row.col(|ui| {
                        let binary = input.source.is_binary();
                        let mut mode = input.mode;
                        let prev = mode;
                        egui::ComboBox::from_id_salt(("mode", input.id))
                            .width(ui.available_width())
                            .selected_text(mode.label())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut mode,
                                    InputBindingMode::Value,
                                    InputBindingMode::Value.label(),
                                );
                                if binary {
                                    ui.selectable_value(
                                        &mut mode,
                                        InputBindingMode::TriggerOnPress,
                                        InputBindingMode::TriggerOnPress.label(),
                                    );
                                    ui.selectable_value(
                                        &mut mode,
                                        InputBindingMode::TriggerOnRelease,
                                        InputBindingMode::TriggerOnRelease.label(),
                                    );
                                }
                            });
                        if mode != prev {
                            set_mode.push((c.id, input.id, mode));
                        }
                    });
                    row.col(|ui| {
                        let v = c.values.get(idx).copied().unwrap_or(0.0);
                        ui.colored_label(
                            hl_text_color.unwrap_or(Color32::from_gray(200)),
                            egui::RichText::new(format!("{:.2}", v))
                                .monospace()
                                .size(11.0),
                        );
                    });
                    // Feedback toggle — only present for feedback-capable
                    // controllers. When unchecked, this mapping is silenced:
                    // no MIDI sent back to the device, no input port on the
                    // Input Controller node.
                    if has_feedback {
                        row.col(|ui| {
                            let mut enabled = !input.disable_feedback;
                            if ui.checkbox(&mut enabled, "")
                                .on_hover_text("Feedback for this mapping")
                                .changed()
                            {
                                set_feedback_disabled.push((c.id, input.id, !enabled));
                            }
                        });
                    }
                    // Learn column — single toggle. When armed, clicking
                    // again cancels. When any row is armed, the next MIDI
                    // message replaces that row's source (handled engine-side).
                    row.col(|ui| {
                        let btn = egui::SelectableLabel::new(this_armed, "⟳");
                        if ui.add(btn)
                            .on_hover_text(if this_armed {
                                "Cancel relearn"
                            } else {
                                "Click, then move the physical control to rebind this row"
                            })
                            .clicked()
                        {
                            let target = if this_armed { None } else { Some(input.id) };
                            set_relearn.push((c.id, target));
                        }
                    });
                    // Delete column — enabled for all kinds. Factory-preset
                    // users sometimes want to trim rows they don't use, or
                    // clean up entries added via Learn. Re-add is always
                    // possible via Learn (generic) or Reset to factory.
                    row.col(|ui| {
                        if ui.small_button(egui_phosphor::regular::X).clicked() {
                            remove_input.push((c.id, input.id));
                        }
                    });
                });
            }
        });
}

fn show_debug_panel(
    ui: &mut Ui,
    c: &crate::input_controller::ControllerRuntime,
    debug_set_out: &mut Vec<(u32, usize, f32)>,
    set_debug_flags: &mut Vec<(u32, Option<bool>, Option<bool>, Option<bool>)>,
    clear_midi_log: &mut Vec<u32>,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        // Flags row.
        let has_feedback = c.kind.has_feedback();
        ui.horizontal_wrapped(|ui| {
            let mut highlight = c.debug_highlight_on_touch;
            if ui.checkbox(&mut highlight, "Highlight on touch").on_hover_text(
                "Jump to and briefly highlight the row in the inputs table when a matching event comes in from the hardware."
            ).changed() {
                set_debug_flags.push((c.id, None, None, Some(highlight)));
            }
            if has_feedback {
                let mut override_flag = c.debug_feedback_override;
                if ui.checkbox(&mut override_flag, "Override feedback").on_hover_text(
                    "Block the graph from writing to the device — the test sliders below take over."
                ).changed() {
                    set_debug_flags.push((c.id, Some(override_flag), None, None));
                }
                let mut loopback = c.debug_loopback;
                if ui.checkbox(&mut loopback, "Loopback to input").on_hover_text(
                    "Also mirror slider writes into the device's input values, so the engine node's output ports reflect them without moving the hardware."
                ).changed() {
                    set_debug_flags.push((c.id, None, Some(loopback), None));
                }
            } else {
                ui.colored_label(
                    Color32::from_gray(140),
                    "No feedback path; test sliders disabled.",
                );
            }
        });

        // Rolling activity log. MIDI-kind controllers prefix each line with
        // the raw message bytes; other kinds (X1, future HID) just show the
        // decoded event, since there's no meaningful wire representation.
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Activity").strong());
            ui.colored_label(Color32::from_gray(140), format!("{} events", c.activity_log.len()));
            if ui.small_button("Clear").clicked() {
                clear_midi_log.push(c.id);
            }
        });
        egui::ScrollArea::vertical()
            .id_salt(("activity_log", c.id))
            .max_height(140.0)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in c.activity_log.iter() {
                    // Prefix is the raw bytes when available, otherwise an
                    // empty column so decoded-only entries line up vertically.
                    let prefix: String = match &entry.raw {
                        Some(bytes) => bytes.iter()
                            .map(|b| format!("{:02X}", b))
                            .collect::<Vec<_>>()
                            .join(" "),
                        None => String::new(),
                    };
                    let (label, color) = match (&entry.decoded, entry.matched_input_idx) {
                        (Some((_, v)), Some(idx)) => {
                            let name = c.inputs.get(idx)
                                .map(|i| i.name.as_str())
                                .unwrap_or("?");
                            (format!("{:<10} → {} = {:.3}", prefix, name, v), Color32::from_gray(220))
                        }
                        (Some((src, v)), None) => {
                            (format!("{:<10} → {} = {:.3} (unmatched)", prefix, src.label(), v),
                             Color32::from_rgb(200, 170, 90))
                        }
                        (None, _) => {
                            (format!("{:<10} (unparsed)", prefix), Color32::from_gray(140))
                        }
                    };
                    ui.colored_label(color, egui::RichText::new(label).monospace().size(11.0));
                }
            });

        // Test sliders.
        if has_feedback && c.debug_feedback_override {
            ui.separator();
            ui.label(egui::RichText::new("Feedback test").strong());
            egui::ScrollArea::vertical()
                .id_salt(("fb_sliders", c.id))
                .max_height(240.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, input) in c.inputs.iter().enumerate() {
                        // Skip sources we can't send back anyway. Relative
                        // MIDI encoders and X1 pots/encoders are read-only on
                        // the hardware side.
                        let sendable = matches!(
                            &input.source,
                            InputSource::Midi(MidiSource::Cc { .. })
                                | InputSource::Midi(MidiSource::Note { .. })
                                | InputSource::Midi(MidiSource::NoteVelocity { .. })
                                | InputSource::Midi(MidiSource::PitchBend { .. })
                                | InputSource::X1(crate::input_controller::x1::X1Source::Button(_))
                        );
                        if !sendable { continue; }
                        let mut v = c.out_values.get(i).copied().unwrap_or(0.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(
                                egui::RichText::new(&input.name).monospace().size(11.0),
                            ).truncate());
                        });
                        if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).step_by(0.01).show_value(true)).changed() {
                            debug_set_out.push((c.id, i, v));
                        }
                    }
                });
        }
    });
}
