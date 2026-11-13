use crate::LightBeatApp;
use crate::UndoAction;
use crate::engine;
use crate::macros;
use crate::latency;
use crate::project;
use crate::theme;
use crate::update;
use crate::version;
use crate::widgets;
use crate::app::macros::MacroDragPayload;
use crate::app::files::PendingAction;
use crate::app::undo::UndoDoc;
use crate::app::shortcuts;
use crate::app::panels::{NodeDragPayload, FrameDragPayload, show_frame_inspector, show_frames_colour_inspector};
use std::path::PathBuf;
use eframe::egui;
use crate::config::InspectorMode;

/// How long after the last input the UI keeps repainting at full rate.
const ACTIVE_REPAINT_WINDOW: f64 = 2.0;

/// Margin before the intended idle frame time, so the wake-up precedes the vblank it targets.
const IDLE_WAKE_EARLY_SECS: f32 = 0.003;

impl LightBeatApp {
    pub(crate) fn update_frame(&mut self, ctx: &egui::Context) {
        self.frame_bookkeeping(ctx);

        self.show_menu_bar(ctx);

        self.update_check_tick();
        self.show_update_ui(ctx);
        self.show_shortcuts_window(ctx);
        self.show_preferences_window(ctx);

        self.show_setup_window(ctx);

        self.input_controllers.reap_lost_sessions();

        // "Add Nodes" side panel (left) — unified node-registry + macro library.
        if self.show_add_nodes {
            egui::SidePanel::left("add_nodes_panel")
                .default_width(220.0)
                .show(ctx, |ui| {
                    self.show_add_nodes_panel(ui);
                });
        }

        // Pump pending macro requests from the graph (right-click actions).
        if let Some(req) = self.graph.take_macro_request() {
            self.handle_macro_request(req);
        }

        self.show_save_macro_dialog(ctx);

        self.show_inspector_panel(ctx);

        // DMX Monitor (floating window, toggled via View menu).
        if self.show_dmx_monitor {
            egui::Window::new("DMX monitor")
                .open(&mut self.show_dmx_monitor)
                .default_size([600.0, 200.0])
                .show(ctx, |ui| {
                    let iface_names: Vec<(u32, String)> = self.interface_manager.interfaces
                        .iter()
                        .map(|e| (e.id, e.name.clone()))
                        .collect();
                    self.dmx_monitor.show(ui, &self.dmx_shared, &iface_names);
                });
        }

        self.show_status_bar(ctx);

        self.show_canvas(ctx);

        self.handle_shortcuts(ctx);

        self.handle_close_and_dialogs(ctx);
    }

    fn frame_bookkeeping(&mut self, ctx: &egui::Context) {
        // Full rate while the user is working, a slower one when a show runs
        // unattended: the canvas is expensive to redraw and a hardware
        // controller changes things without touching the window.
        let now = ctx.input(|i| i.time);
        let idle_hz = self.config.idle_repaint_hz;
        if idle_hz <= 0.0 || now - self.last_interaction_time < ACTIVE_REPAINT_WINDOW {
            ctx.request_repaint();
        } else {
            // egui subtracts its predicted frame time from the delay so as not
            // to overshoot, which on a vsynced display turns a 33 ms request
            // into the next vsync, i.e. no slowdown at all. Add it back, and
            // wake a little early so the swap lands on the intended vblank.
            let predicted = ctx.input(|i| i.predicted_dt);
            let wait = (1.0 / idle_hz - IDLE_WAKE_EARLY_SECS).max(0.0) + predicted;
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(wait));
        }
        // Only on change: the command reaches the window system every time
        // it is sent, and with client-side decorations it redraws the title bar.
        let title = self.window_title();
        if title != self.last_window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_window_title = title;
        }

        // Remember which panels are open; only written when one changed.
        let panels_changed = self.config.show_add_nodes != self.show_add_nodes
            || self.config.show_setup != self.show_setup
            || self.config.show_dmx_monitor != self.show_dmx_monitor
            || self.config.setup_tab != self.setup_active_tab;
        if panels_changed {
            self.config.show_add_nodes = self.show_add_nodes;
            self.config.show_setup = self.show_setup;
            self.config.show_dmx_monitor = self.show_dmx_monitor;
            self.config.setup_tab = self.setup_active_tab;
            self.save_settings();
        }

        // Track window size / maximized state so on_exit can persist them.
        // Only record size when *not* maximized, so toggling maximize off on
        // next launch restores the user's last manual size.
        ctx.input(|i| {
            let v = i.viewport();
            let maximized = v.maximized.unwrap_or(false);
            self.config.window_maximized = maximized;
            if !maximized
                && let Some(rect) = v.inner_rect {
                    self.config.window_size = Some((rect.width(), rect.height()));
                }
        });

        // Poll background file dialog results.
        self.poll_file_dialog(ctx);

        // Snapshot current state for the undoers and the Edit menu.
        // Drain any "save current as preset" requests from Gradient Source
        // widgets into the preset manager, then re-sync the shared mirror.
        self.drain_pending_gradient_saves();
        self.check_engine_health();
        self.report_settings_failure();
        // Keep the shared library in sync with the manager every frame —
        // the management window edits `gradient_preset_manager.presets`
        // directly (rename / delete / add), and those changes need to be
        // visible to the Gradient Source dropdowns.
        self.sync_gradient_library();

        let now = ctx.input(|i| i.time);

        // Undo bookkeeping. This used to rebuild the whole project and setup
        // every frame — a deep clone of every node's params and save_data plus
        // several deep comparisons, all scaling with project size rather than
        // with what is on screen.
        //
        // Neither can change except as a result of user input, so snapshots
        // are gated on having had input recently. Pointer movement alone is
        // ignored: hovering the canvas mutates nothing. The window has to
        // outlast `Undoer::stable_time` (1s) because the Undoer only commits
        // an undo point after seeing an unchanged state for that long.
        const UNDO_SETTLE_WINDOW: f64 = 2.0;
        // Bare pointer motion mutates nothing, and egui reports it as two
        // separate events. Anything not on this list counts as interaction,
        // so an unrecognised event errs towards taking a snapshot.
        let interacted = ctx.input(|i| {
            i.pointer.any_down()
                || i.events.iter().any(|e| {
                    !matches!(
                        e,
                        egui::Event::PointerMoved(_)
                            | egui::Event::MouseMoved(_)
                            | egui::Event::PointerGone
                    )
                })
        });
        if interacted {
            self.last_interaction_time = now;
        }
        self.ui_timers
            .tick_frame(&self.perf, now, ctx.input(|i| i.unstable_dt));
        // Within the window the Undoer only needs to see the state often
        // enough to judge stability against its 1 s threshold, so feeding it
        // at 10 Hz instead of every frame loses nothing.
        const SNAPSHOT_MIN_INTERVAL: f64 = 0.1;
        let in_window = now - self.last_interaction_time < UNDO_SETTLE_WINDOW;
        engine::sync_hint::set_interacting(in_window);
        let due = now - self.last_snapshot_time >= SNAPSHOT_MIN_INTERVAL;
        if in_window && due {
            self.last_snapshot_time = now;
            let started = std::time::Instant::now();
            let current_project = project::save_graph(&self.graph);
            let current_setup = self.current_setup();
            self.ui_timers
                .record_snapshot(&self.perf, started.elapsed().as_micros() as u64);

            if current_setup != self.setup_baseline {
                self.setup_baseline = current_setup.clone();
                self.setup_changed_at = Some(now);
                self.last_changed_doc = Some(UndoDoc::Setup);
            }
            // The first sight of the project is the baseline, not an edit.
            if let Some(base) = &self.project_baseline
                && *base != current_project
            {
                self.last_changed_doc = Some(UndoDoc::Project);
            }
            if self.project_baseline.as_ref() != Some(&current_project) {
                self.project_baseline = Some(current_project.clone());
            }
            self.project_undoer.feed_state(now, &current_project);
            self.setup_undoer.feed_state(now, &current_setup);
            self.can_undo_proj = self.project_undoer.has_undo(&current_project);
            self.can_redo_proj = self.project_undoer.has_redo(&current_project);
            self.can_undo_setup = self.setup_undoer.has_undo(&current_setup);
            self.can_redo_setup = self.setup_undoer.has_redo(&current_setup);
        }

        // The setup is hardware configuration that is easy to lose in a crash,
        // so it is written shortly after the last change instead of only on exit.
        const SETUP_SAVE_DELAY: f64 = 2.0;
        if let Some(t) = self.setup_changed_at
            && now - t >= SETUP_SAVE_DELAY
        {
            self.save_setup();
        }

        // Periodic crash-recovery autosave: writes to a separate file, never
        // over the user's project. Only runs when there are unsaved changes.
        const AUTOSAVE_INTERVAL: f64 = 30.0;
        if self.config.autosave_enabled
            && now - self.last_autosave_time > AUTOSAVE_INTERVAL
            && self.is_project_dirty()
        {
            self.write_autosave();
            self.last_autosave_time = now;
        }
    }

    fn show_menu_bar(&mut self, ctx: &egui::Context) {
        let undo_action = self.undo_action(false);
        let redo_action = self.undo_action(true);

        if let Some(lines) = self.setup_notice.clone() {
            let mut open = true;
            let mut dismiss = false;
            egui::Window::new("Import / export setup").open(&mut open).collapsible(false).resizable(true).show(ctx, |ui| {
                for line in &lines {
                    ui.label(line);
                }
                ui.add_space(6.0);
                if ui.button("OK").clicked() {
                    dismiss = true;
                }
            });
            if !open || dismiss {
                self.setup_notice = None;
            }
        }

        // Menu bar.
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        ui.close_menu();
                        self.request_action(ctx, PendingAction::New);
                    }
                    if ui.add(egui::Button::new("Open...").shortcut_text(ctx.format_shortcut(&shortcuts::OPEN))).clicked() {
                        ui.close_menu();
                        self.request_action(ctx, PendingAction::Open);
                    }
                    let recents = self.config.recent_projects.clone();
                    let mut load_recent: Option<PathBuf> = None;
                    let mut clear_recents = false;
                    ui.add_enabled_ui(!recents.is_empty(), |ui| {
                        ui.menu_button("Open recent", |ui| {
                            for p in &recents {
                                let label = p.file_name()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or_else(|| p.to_str().unwrap_or("?"))
                                    .to_string();
                                if ui.button(label)
                                    .on_hover_text(p.to_string_lossy())
                                    .clicked()
                                {
                                    load_recent = Some(p.clone());
                                    ui.close_menu();
                                }
                            }
                            ui.separator();
                            if ui.button("Clear list").clicked() {
                                clear_recents = true;
                                ui.close_menu();
                            }
                        });
                    });
                    if let Some(p) = load_recent {
                        self.request_action(ctx, PendingAction::OpenPath(p));
                    }
                    if clear_recents {
                        self.config.recent_projects.clear();
                        self.save_settings();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Save").shortcut_text(ctx.format_shortcut(&shortcuts::SAVE))).clicked() {
                        ui.close_menu();
                        self.save_command(ctx, None);
                    }
                    if ui.button("Save as...").clicked() {
                        ui.close_menu();
                        self.save_project_as(ctx);
                    }
                    ui.separator();
                    if ui.button("Export setup...").on_hover_text("Fixtures, objects, groups, interfaces, palettes, controllers and audio inputs, as one file").clicked() {
                        ui.close_menu();
                        self.export_setup_dialog(ctx);
                    }
                    if ui.button("Import setup...").on_hover_text("Replace the setup with one from a file; Edit, Undo (setup) brings the old one back").clicked() {
                        ui.close_menu();
                        self.import_setup_dialog(ctx);
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Quit").shortcut_text(ctx.format_shortcut(&shortcuts::QUIT))).clicked() {
                        ui.close_menu();
                        self.quit_requested = true;
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo_label = match undo_action {
                        Some(UndoAction::UndoProject) => "Undo (project)",
                        Some(UndoAction::UndoSetup) => "Undo (setup)",
                        _ => "Undo",
                    };
                    let redo_label = match redo_action {
                        Some(UndoAction::RedoProject) => "Redo (project)",
                        Some(UndoAction::RedoSetup) => "Redo (setup)",
                        _ => "Redo",
                    };
                    if ui.add_enabled(undo_action.is_some(), egui::Button::new(undo_label).shortcut_text(ctx.format_shortcut(&shortcuts::UNDO))).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = undo_action;
                    }
                    if ui.add_enabled(redo_action.is_some(), egui::Button::new(redo_label).shortcut_text(ctx.format_shortcut(&shortcuts::REDO))).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = redo_action;
                    }
                    ui.separator();
                    if ui.button("Preferences...").clicked() {
                        ui.close_menu();
                        self.show_preferences = true;
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_add_nodes, "Add nodes").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_setup, "Setup").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_dmx_monitor, "DMX monitor").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.config.snap_to_grid, "Snap to grid").changed() {
                        self.save_settings();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("Inspector", |ui| {
                        let mut changed = false;
                        if ui.radio_value(&mut self.config.inspector_mode, InspectorMode::Show, "Show").clicked() {
                            changed = true;
                        }
                        if ui.radio_value(&mut self.config.inspector_mode, InspectorMode::Auto, "Auto").clicked() {
                            changed = true;
                        }
                        if ui.radio_value(&mut self.config.inspector_mode, InspectorMode::Hide, "Hide").clicked() {
                            changed = true;
                        }
                        if changed { self.save_settings(); }
                    });
                });
                ui.menu_button("Help", |ui| {
                    if ui.button(format!("About LightBeat {}...", version::version())).clicked() {
                        ui.close_menu();
                        self.show_about = true;
                    }
                    if ui.button("Keyboard shortcuts...").clicked() {
                        ui.close_menu();
                        self.show_shortcuts = true;
                    }
                    ui.separator();
                    if ui.button("Check for updates...").clicked() {
                        ui.close_menu();
                        self.show_update_window = true;
                        self.updater.check(self.config.update_channel);
                        self.last_update_check = Some(std::time::Instant::now());
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(theme::TEXT_DIM, format!("LightBeat {}", version::version()));
                });
            });
        });
    }

    fn show_setup_window(&mut self, ctx: &egui::Context) {
        // Consolidated Setup window — embedded `egui::Window` inside the
        // main viewport. All setup managers (interfaces, fixtures, objects,
        // groups, palettes, gradients, audio inputs, input controllers)
        // live as tabs inside one window.
        if self.show_setup {
            // Stage the open flag locally so the closure can borrow `self`
            // mutably; assign back after the window returns.
            let mut open = true;
            egui::Window::new("Setup")
                .open(&mut open)
                .default_size([780.0, 560.0])
                .min_width(420.0)
                .min_height(320.0)
                .show(ctx, |ui| {
                    self.show_setup_tabs(ui);
                });
            self.show_setup = open;
        }
    }

    fn show_save_macro_dialog(&mut self, ctx: &egui::Context) {
        // Save-as-macro modal dialog.
        let mut close_dialog = false;
        let mut save_action: Option<()> = None;
        if let Some(dlg) = &mut self.save_macro_dialog {
            // Snapshot the library entries for the "Update existing" dropdown.
            // Cloning is cheap (metadata only — graph content loads on demand).
            let library_entries: Vec<macros::library::MacroEntry> = self.library.entries.clone();

            let mut open = true;
            egui::Window::new("Save as macro")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .default_size([360.0, 0.0])
                .show(ctx, |ui| {
                    let overwriting = dlg.overwrite_path.is_some();
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let selected_text = match &dlg.overwrite_path {
                            Some(p) => p
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("(existing)")
                                .to_string(),
                            None => "New macro".to_string(),
                        };
                        egui::ComboBox::from_id_salt("save_macro_target")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(dlg.overwrite_path.is_none(), "New macro").clicked() {
                                    dlg.overwrite_path = None;
                                }
                                for entry in &library_entries {
                                    let picked = dlg.overwrite_path.as_deref() == Some(entry.path.as_path());
                                    let label = if entry.group.is_empty() {
                                        entry.name.clone()
                                    } else {
                                        format!("{} / {}", entry.group, entry.name)
                                    };
                                    if ui.selectable_label(picked, label).clicked() {
                                        dlg.name = entry.name.clone();
                                        dlg.group = entry.group.clone();
                                        dlg.description = entry.description.clone();
                                        dlg.tags = entry.tags.join(", ");
                                        dlg.overwrite_path = Some(entry.path.clone());
                                    }
                                }
                            });
                    });
                    ui.separator();

                    egui::Grid::new("save_macro_grid")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Name:");
                            ui.add_enabled(!overwriting, egui::TextEdit::singleline(&mut dlg.name));
                            ui.end_row();
                            ui.label("Group:");
                            ui.add_enabled(!overwriting, egui::TextEdit::singleline(&mut dlg.group))
                                .on_hover_text("Optional. Use '/' for nested groups, e.g. \"audio/triggers\".");
                            ui.end_row();
                            ui.label("Tags:");
                            ui.text_edit_singleline(&mut dlg.tags)
                                .on_hover_text("Comma-separated.");
                            ui.end_row();
                            ui.label("Description:");
                            ui.text_edit_multiline(&mut dlg.description);
                            ui.end_row();
                        });
                    if let Some(err) = &dlg.error {
                        ui.colored_label(theme::SEM_DANGER, err);
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        let can_save = !dlg.name.trim().is_empty();
                        let save_label = if overwriting { "Update" } else { "Save" };
                        if ui.add_enabled(can_save, egui::Button::new(save_label)).clicked() {
                            save_action = Some(());
                        }
                        if ui.button("Cancel").clicked() {
                            close_dialog = true;
                        }
                    });
                });
            if !open { close_dialog = true; }
        }
        if save_action.is_some() {
            if let Err(e) = self.save_pending_macro() {
                if let Some(dlg) = &mut self.save_macro_dialog {
                    dlg.error = Some(e);
                }
            } else {
                close_dialog = true;
            }
        }
        if close_dialog { self.save_macro_dialog = None; }
    }

    fn show_inspector_panel(&mut self, ctx: &egui::Context) {
        // Inspector panel — visibility gated by InspectorMode.
        let frame_selected_count = self.graph.selected_frame_ids().count();
        let has_selection = !self.graph.selected_nodes_mut().is_empty()
            || frame_selected_count > 0;
        let show_inspector = match self.config.inspector_mode {
            InspectorMode::Show => true,
            InspectorMode::Hide => false,
            InspectorMode::Auto => has_selection,
        };
        if show_inspector {
            egui::SidePanel::right("inspector")
                .default_width(250.0)
                .show(ctx, |ui| {
                    // The snapshot list lives at the bottom of the panel with a
                    // handle to size it; the inspector keeps the rest.
                    if self.graph.selected_frame_ids().next().is_none() {
                        let mut selected = self.graph.selected_nodes_mut();
                        if selected.len() == 1 {
                            let node = &mut *selected[0];
                            if widgets::snapshots::applicable(node.as_ref()) {
                                egui::TopBottomPanel::bottom("snapshot_panel")
                                    .resizable(true)
                                    .default_height(220.0)
                                    .height_range(110.0..=700.0)
                                    .show_inside(ui, |ui| widgets::snapshots::show(ui, node.as_ref()));
                            }
                        }
                    }
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            // Frames first — when a single frame is selected (and no
                            // nodes), show the frame's editable title/color/notes.
                            let selected_frame_ids: Vec<u64> = self.graph.selected_frame_ids().collect();
                            let nodes_selected = !self.graph.selected_nodes_mut().is_empty();

                            if !nodes_selected && selected_frame_ids.len() == 1 {
                                show_frame_inspector(ui, &mut self.graph, selected_frame_ids[0]);
                                return;
                            }
                            if !nodes_selected && selected_frame_ids.len() > 1 {
                                show_frames_colour_inspector(ui, &mut self.graph, &selected_frame_ids);
                                return;
                            }

                            let selected = self.graph.selected_nodes_mut();
                            if selected.is_empty() {
                                ui.heading("Inspector");
                                ui.separator();
                                ui.label("Select a node or frame to inspect.");
                            } else if selected.len() == 1 {
                                let node = &mut *selected.into_iter().next().unwrap();
                                widgets::inspector::show_inspector(ui, node.as_mut());
                            } else {
                                ui.heading(format!("{} nodes selected", selected.len()));
                                ui.separator();
                                widgets::inspector::show_multi_inspector(ui, selected);
                            }
                        });
                });
        }
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
        // Status bar.
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let mut shared = self.dmx_shared.lock();

                    // Blackout button — red when active.
                    let blackout_text = if shared.blackout {
                        egui::RichText::new("BLACKOUT").color(theme::TEXT_BRIGHT).strong()
                    } else {
                        egui::RichText::new("Blackout").color(theme::TEXT_DIM)
                    };
                    let blackout_btn = egui::Button::new(blackout_text)
                        .fill(if shared.blackout { theme::SEM_DANGER } else { theme::CLEAR });
                    if ui.add(blackout_btn).clicked() {
                        shared.blackout = !shared.blackout;
                    }

                    ui.separator();

                    // Bypass button — orange when active.
                    let bypass_text = if shared.bypass {
                        egui::RichText::new("BYPASS").color(theme::TEXT_BRIGHT).strong()
                    } else {
                        egui::RichText::new("Bypass").color(theme::TEXT_DIM)
                    };
                    let bypass_btn = egui::Button::new(bypass_text)
                        .fill(if shared.bypass { theme::SEM_WARNING } else { theme::CLEAR });
                    if ui.add(bypass_btn).clicked() {
                        shared.bypass = !shared.bypass;
                    }

                    ui.separator();

                    // Status text.
                    let status = if shared.blackout {
                        "All outputs zeroed"
                    } else if shared.bypass {
                        "DMX output suspended"
                    } else {
                        "Live"
                    };
                    ui.colored_label(theme::TEXT_DIM, status);

                    if !self.engine.is_running() {
                        ui.separator();
                        ui.colored_label(theme::SEM_DANGER, egui::RichText::new("Engine stopped").strong())
                            .on_hover_text("The engine thread has stopped, so the output is frozen at its last look. Save the project and restart the app.");
                    }
                    let dropped = self.engine.dropped_commands();
                    if dropped > 0 {
                        ui.separator();
                        ui.colored_label(theme::SEM_WARNING, format!("Engine busy: {dropped} changes lost"))
                            .on_hover_text("The engine could not keep up and dropped some changes, so the graph on screen may differ from what runs. Reload the project to be safe.");
                    }

                    let missing = {
                        let mut m = self.input_controllers.missing_controllers();
                        m.extend(self.audio_inputs.missing_inputs());
                        m.extend(self.midi_hub.missing_devices());
                        m
                    };
                    if let Some(text) = widgets::setup_ui::not_found_text(&missing) {
                        ui.separator();
                        ui.colored_label(theme::SEM_WARNING, text);
                        if ui
                            .small_button("Rescan")
                            .on_hover_text("Look again for everything listed as not found: controllers, audio inputs and MIDI devices")
                            .clicked()
                        {
                            self.input_controllers.rescan();
                            self.audio_inputs.force_rescan();
                            self.midi_hub.rescan();
                        }
                    }

                    ui.separator();
                    ui.colored_label(theme::TEXT_DIM, "Latency");
                    let mut ms = self.config.latency_ms;
                    let drag = egui::DragValue::new(&mut ms)
                        .speed(0.5)
                        .range(-latency::LIMIT_MS..=latency::LIMIT_MS)
                        .suffix(" ms")
                        .fixed_decimals(0);
                    let drag = ui.add(drag).on_hover_text(
                        "Runs every clock (audio beat clock, Link, internal) this far ahead of the music, to make up for the delay to the lights. Set it once for the room.",
                    );
                    if drag.changed() {
                        self.config.latency_ms = ms;
                        latency::set_lead_ms(ms);
                    }
                    if drag.drag_stopped() || drag.lost_focus() {
                        if let Err(e) = self.config.save() { self.settings_save_error = Some(e); }
                    }

                    match self.updater.state() {
                        update::State::Available(release) => {
                            ui.separator();
                            if ui
                                .small_button(egui::RichText::new(format!("Update available: {}", release.tag)).color(theme::SEM_PRIMARY))
                                .clicked()
                            {
                                self.show_update_window = true;
                            }
                        }
                        update::State::Installed(_) => {
                            ui.separator();
                            if ui.small_button(egui::RichText::new("Updated: restart").color(theme::SEM_SUCCESS)).clicked() {
                                self.show_update_window = true;
                            }
                        }
                        _ => {}
                    }

                    // Right-aligned zoom indicator + performance readout.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(
                            theme::TEXT_DIM,
                            format!("Zoom: {:.0}%", self.graph.zoom() * 100.0),
                        );
                        ui.separator();

                        let (tick_us, tick_max_us) = self.perf.engine_tick.read();
                        let (snap_us, snap_max_us) = self.perf.ui_snapshot.read();
                        let snaps_per_sec = self.perf.snapshots_per_sec();
                        let (frame_us, frame_max_us) = self.perf.ui_frame.read();
                        let (graph_us, _) = self.perf.ui_graph.read();
                        let interval_us = self.perf.frame_interval_us();
                        let fps = if interval_us > 0 { 1_000_000 / interval_us } else { 0 };
                        // The engine tick has a 1 ms budget; warn once the
                        // average eats more than half of it.
                        let tick_color = if tick_us > 800 {
                            theme::SEM_DANGER
                        } else if tick_us > 500 {
                            theme::SEM_WARNING
                        } else {
                            theme::TEXT_DIM
                        };
                        let perf_text = format!(
                            "engine {}/{} us  ·  ui {:.1}/{:.1} ms (graph {:.1}) @ {} fps  ·  undo snap {:.1}/{:.1} ms @ {}/s",
                            tick_us,
                            tick_max_us,
                            frame_us as f64 / 1000.0,
                            frame_max_us as f64 / 1000.0,
                            graph_us as f64 / 1000.0,
                            fps,
                            snap_us as f64 / 1000.0,
                            snap_max_us as f64 / 1000.0,
                            snaps_per_sec,
                        );
                        let resp = ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(perf_text).color(tick_color),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text(
                                "Engine tick average/peak (1000 us budget); UI frame average/peak \
                                 with the node graph canvas's share and the real frame rate; \
                                 undo-snapshot average/peak with snapshots taken per second. \
                                 Click to reset peaks.",
                            );
                        if resp.clicked() {
                            self.perf.reset_peaks();
                        }
                    });
                });
            });
    }

    fn show_canvas(&mut self, ctx: &egui::Context) {
        // Node graph.
        let graph_started = std::time::Instant::now();
        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui, self.config.snap_to_grid);
        });
        self.ui_timers
            .record_graph(&self.perf, graph_started.elapsed().as_micros() as u64);

        // Accept drops from the Add Nodes panel onto the canvas. We only
        // consume the payload when the pointer is released inside the canvas
        // rect — otherwise egui keeps the drag alive and the user can
        // continue aiming. `take_payload` always removes the payload even
        // when the type doesn't match, so dispatch via the non-consuming
        // `has_payload_of_type` first and only `take_payload` the right one.
        if ctx.input(|i| i.pointer.any_released())
            && let Some(pos) = ctx.pointer_interact_pos()
            && self.graph.canvas_rect().contains(pos) {
                let world = self.graph.screen_to_world(pos);
                if egui::DragAndDrop::has_payload_of_type::<MacroDragPayload>(ctx) {
                    if let Some(p) = egui::DragAndDrop::take_payload::<MacroDragPayload>(ctx) {
                        self.instantiate_macro_from_path(&p.path, world);
                    }
                } else if egui::DragAndDrop::has_payload_of_type::<NodeDragPayload>(ctx) {
                    if let Some(p) = egui::DragAndDrop::take_payload::<NodeDragPayload>(ctx) {
                        self.graph.spawn_from_registry(p.registry_idx, world);
                    }
                } else if egui::DragAndDrop::has_payload_of_type::<FrameDragPayload>(ctx) {
                    let _ = egui::DragAndDrop::take_payload::<FrameDragPayload>(ctx);
                    self.graph.add_frame_at(world);
                }
            }

        self.wire_new_nodes();

        // Re-sync interfaces if they changed.
        if self.interface_manager.needs_sync {
            self.interface_manager.needs_sync = false;
            self.sync_interfaces();
        }

        // Sync object store if objects were edited directly.
        if self.object_manager.needs_sync {
            self.object_manager.needs_sync = false;
            self.sync_object_store();
        }

        // Re-register group nodes and sync objects if groups/objects changed.
        if self.group_manager.needs_refresh {
            self.group_manager.needs_refresh = false;
            self.sync_group_context();
            self.sync_object_store();
        }

        if self.color_palette_manager.needs_refresh || self.color_palette_group_manager.needs_refresh {
            self.color_palette_manager.needs_refresh = false;
            self.color_palette_group_manager.needs_refresh = false;
            self.sync_palette_context();
        }

        // Send any pending engine commands from the graph.
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Keyboard shortcuts. `command` is Ctrl on Linux and Cmd on macOS. While a
        // text field has focus the keys belong to it (Ctrl+Z undoes its text),
        // except Ctrl+S, which saving does not conflict with.
        let typing = ctx.wants_keyboard_input();
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_command(ctx, None);
        }
        if !typing && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O)) {
            self.request_action(ctx, PendingAction::Open);
        }
        if !typing && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Q)) {
            self.quit_requested = true;
        }

        // Undo / Redo: Ctrl+Z / Ctrl+Shift+Z (and Ctrl+Y for redo).
        // Acts on whichever of project and setup changed last.
        let undo_pressed = !typing && ctx.input(|i| {
            i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z)
        });
        let redo_pressed = !typing && ctx.input(|i| {
            i.modifiers.command
                && ((i.modifiers.shift && i.key_pressed(egui::Key::Z))
                    || i.key_pressed(egui::Key::Y))
        });

        if undo_pressed {
            self.pending_undo_action = self.undo_action(false);
        }
        if redo_pressed {
            self.pending_undo_action = self.undo_action(true);
        }
        if let Some(action) = self.pending_undo_action.take() {
            self.run_undo_action(action);
        }
    }

    fn handle_close_and_dialogs(&mut self, ctx: &egui::Context) {
        // Intercept the OS window close button: if the project has unsaved
        // changes, cancel the close and open the confirm modal instead.
        if !self.close_discarded
            && ctx.input(|i| i.viewport().close_requested())
            && self.is_project_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_requested = true;
        }

        if self.quit_requested {
            self.quit_requested = false;
            self.request_action(ctx, PendingAction::Quit);
        }

        // Unsaved-changes modal: Save / Discard / Cancel, for quitting, New and Open.
        if let Some(action) = self.unsaved_prompt.clone() {
            let mut do_save = false;
            let mut do_discard = false;
            let mut do_cancel = false;
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("You have unsaved changes in this project. Save before {}?", action.describe()));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new("Save").shortcut_text(ctx.format_shortcut(&shortcuts::SAVE))).clicked() { do_save = true; }
                        if ui.button("Discard").clicked() { do_discard = true; }
                        if ui.button("Cancel").clicked() { do_cancel = true; }
                    });
                });
            if do_save {
                self.unsaved_prompt = None;
                self.save_command(ctx, Some(action));
            } else if do_discard {
                self.unsaved_prompt = None;
                // Opening through the file dialog may still be cancelled, in
                // which case the autosave is the only copy of the edits.
                if !matches!(action, PendingAction::Open) {
                    self.delete_autosave_for(self.project_path.as_deref());
                }
                if matches!(action, PendingAction::Quit) {
                    self.close_discarded = true;
                }
                self.run_action(ctx, action);
            } else if do_cancel {
                self.unsaved_prompt = None;
            }
        }

        if let Some(path) = self.confirm_delete_macro.clone() {
            let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut do_delete = false;
            let mut do_cancel = false;
            egui::Window::new("Delete macro")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(format!("Delete \"{name}\" from the library? The file is removed and undo does not bring it back."));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() { do_delete = true; }
                        if ui.button("Cancel").clicked() { do_cancel = true; }
                    });
                });
            if do_delete {
                self.confirm_delete_macro = None;
                if let Err(e) = self.library.delete(&path) {
                    self.report_error(format!("Could not delete the macro {}: {e}", path.display()));
                }
            } else if do_cancel {
                self.confirm_delete_macro = None;
            }
        }

        if !self.notices.is_empty() {
            let mut dismiss = false;
            egui::Window::new("File problems")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-8.0, -36.0))
                .show(ctx, |ui| {
                    ui.set_max_width(380.0);
                    for line in &self.notices {
                        ui.add(egui::Label::new(egui::RichText::new(line).color(theme::SEM_DANGER)).wrap());
                    }
                    ui.add_space(4.0);
                    if ui.button("Dismiss").clicked() { dismiss = true; }
                });
            if dismiss {
                self.notices.clear();
            }
        }

        // Recovery prompt shown on startup when a newer autosave file exists
        // alongside the loaded project.
        if self.recover_prompt.is_some() {
            let mut do_recover = false;
            let mut do_discard = false;
            let autosave_display = self.recover_prompt
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            egui::Window::new("Recover unsaved changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("An autosave file exists that is newer than the project on disk.");
                    ui.label("This usually means the app was closed unexpectedly.");
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(&autosave_display).monospace().small());
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Recover").clicked() { do_recover = true; }
                        if ui.button("Discard").clicked() { do_discard = true; }
                    });
                });
            if do_recover
                && let Some(autosave) = self.recover_prompt.take() {
                    match project::load_from_file(&autosave) {
                        Ok(proj) => {
                            self.apply_project(&proj);
                            self.graph.fit_to_content();
                            // Keep project_path pointing at the real project:
                            // recovering from autosave should NOT silently save
                            // over the project file — user has to hit Save.
                            self.project_undoer = egui::util::undoer::Undoer::default();
                            // Recovered content is "unsaved" — leave the
                            // baseline pointing at the on-disk project so the
                            // dirty flag reads true until the user saves.
                        }
                        Err(e) => self.report_error(format!("Could not load the autosave: {e}")),
                    }
                }
            if do_discard
                && let Some(autosave) = self.recover_prompt.take() {
                    let _ = std::fs::remove_file(&autosave);
                }
        }
    }
}
