use crate::LightBeatApp;
use crate::theme;
use crate::widgets;
use crate::app::macros::MacroDragPayload;
use eframe::egui;

/// Active tab in the consolidated Setup window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum SetupTab {
    #[default]
    Interfaces,
    FixtureTemplates,
    Objects,
    Groups,
    ColorPalettes,
    ColorPaletteGroups,
    Gradients,
    AudioInputs,
    InputControllers,
    DmxInputs,
}

const SETUP_TABS: &[(SetupTab, &str)] = &[
    (SetupTab::Interfaces, "Interfaces"),
    (SetupTab::FixtureTemplates, "Fixture templates"),
    (SetupTab::Objects, "Objects"),
    (SetupTab::Groups, "Groups"),
    (SetupTab::ColorPalettes, "Color palettes"),
    (SetupTab::ColorPaletteGroups, "Palette groups"),
    (SetupTab::Gradients, "Gradient presets"),
    (SetupTab::AudioInputs, "Audio inputs"),
    (SetupTab::InputControllers, "Input controllers"),
    (SetupTab::DmxInputs, "DMX inputs"),
];

/// Payload carried during a node-registry → canvas drag (Add Nodes panel).
#[derive(Clone)]
pub(crate) struct NodeDragPayload {
    pub(crate) registry_idx: usize,
}

/// Payload carried during a frame → canvas drag (Add Nodes panel).
#[derive(Clone)]
pub(crate) struct FrameDragPayload;

impl LightBeatApp {
    pub(crate) fn show_add_nodes_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Add nodes");
            if ui.small_button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh macro library")
                .clicked()
            {
                self.library.rescan();
            }
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add(egui::TextEdit::singleline(&mut self.library_search)
                .hint_text("Search nodes, macros, tags...")
                .desired_width(ui.available_width()));
        });
        ui.separator();

        if let Some(err) = &self.library.last_error {
            ui.colored_label(theme::SEM_DANGER, err);
        }

        let q = self.library_search.trim().to_lowercase();

        // Registered node types: grouped by category when nothing is typed, in
        // quick add's ranking (one flat list) when searching. Hidden
        // categories (prefixed with `_`) are always excluded.
        let searching = !q.is_empty();
        let order: Vec<usize> = if searching {
            self.graph.find_entries(&q)
        } else {
            (0..self.graph.registry_entries().len()).collect()
        };
        let node_entries: Vec<(usize, String, String, &'static str)> = order
            .into_iter()
            .map(|i| (i, &self.graph.registry_entries()[i]))
            .filter(|(_, e)| !e.category.starts_with('_'))
            .map(|(i, e)| (i, e.category.clone(), e.label.clone(), e.description))
            .collect();

        // A click adds at the middle of what the canvas shows; dragging places it.
        let centre = self.graph.screen_to_world(self.graph.canvas_rect().center());

        let macro_entries: Vec<_> = self.library.entries.iter().enumerate()
            .filter(|(_, e)| {
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.group.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
                    || "macros".contains(&q)
            })
            .map(|(i, e)| (i, e.clone()))
            .collect();

        // "Frame" is a pseudo-entry so it takes part in search the same way.
        let frame_matches = q.is_empty()
            || "frame".contains(&q)
            || "decorative".contains(&q);

        let mut delete: Option<std::path::PathBuf> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // --- Decorative (Frame) ---
            if frame_matches {
                ui.colored_label(
                    theme::TEXT_DIM,
                    egui::RichText::new("Decorative").strong().size(11.0),
                );
                let dnd_id = egui::Id::new("frame-dnd");
                let resp = ui.dnd_drag_source(dnd_id, FrameDragPayload, |ui| {
                    ui.add(egui::Button::new("+ Frame")
                        .wrap_mode(egui::TextWrapMode::Extend))
                }).inner;
                if resp.clicked() {
                    self.graph.add_frame_at(centre);
                }
                resp.on_hover_text(
                    "Click or drag onto canvas to add a decorative frame for visually \
                     grouping nodes. Drag the title bar to move, the bottom-\
                     right corner to resize.",
                );
                ui.add_space(4.0);
            }

            // --- Registered node types, grouped by category ---
            let mut last_cat = "";
            for (idx, cat, label, desc) in &node_entries {
                if !searching && cat != last_cat {
                    if !last_cat.is_empty() { ui.add_space(4.0); }
                    ui.colored_label(
                        theme::TEXT_DIM,
                        egui::RichText::new(cat).strong().size(11.0),
                    );
                    last_cat = cat;
                }
                let dnd_id = egui::Id::new(("node-dnd", *idx));
                let payload = NodeDragPayload { registry_idx: *idx };
                let resp = ui.dnd_drag_source(dnd_id, payload, |ui| {
                    ui.add(egui::Button::new(label.as_str())
                        .wrap_mode(egui::TextWrapMode::Extend))
                }).inner;
                if resp.clicked() {
                    self.graph.spawn_from_registry(*idx, centre);
                }
                let mut hover = "Click or drag onto canvas to add.".to_string();
                if searching {
                    hover.push_str(&format!("\n\n{cat}"));
                }
                if !desc.is_empty() {
                    hover.push_str(&format!("\n\n{desc}"));
                }
                resp.on_hover_text(hover);
            }

            // --- Macros, grouped by their on-disk group path ---
            if !macro_entries.is_empty() {
                if !node_entries.is_empty() || frame_matches { ui.add_space(4.0); }
                ui.colored_label(
                    theme::ACCENT_MACRO,
                    egui::RichText::new("Macros").strong().size(11.0),
                );
                // Match the title-bar tint macro nodes get in the graph.
                let macro_fill = theme::mix_color(theme::ACCENT_MACRO, theme::BG_HIGH, 0.7);
                let mut current_group: Option<String> = None;
                for (_idx, e) in &macro_entries {
                    if Some(&e.group) != current_group.as_ref() {
                        if current_group.is_some() { ui.add_space(2.0); }
                        let label = if e.group.is_empty() { "(root)".to_string() } else { e.group.clone() };
                        ui.colored_label(
                            theme::TEXT_DIM,
                            egui::RichText::new(label).size(10.0),
                        );
                        current_group = Some(e.group.clone());
                    }
                    let dnd_id = egui::Id::new(("macro-dnd", &e.path));
                    let payload = MacroDragPayload { path: e.path.clone() };
                    let inner_resp = ui.dnd_drag_source(dnd_id, payload, |ui| {
                        ui.add(
                            egui::Button::new(egui::RichText::new(&e.name).color(theme::TEXT_BRIGHT))
                                .fill(macro_fill)
                                .wrap_mode(egui::TextWrapMode::Extend)
                        )
                    });
                    let mut hover = format!("Click or drag onto canvas to add.\n\n{}", e.description);
                    if !e.tags.is_empty() {
                        hover.push_str(&format!("\n\nTags: {}", e.tags.join(", ")));
                    }
                    let btn_resp = inner_resp.inner.on_hover_text(hover);
                    if btn_resp.clicked() {
                        self.instantiate_macro_from_path(&e.path, centre);
                    }
                    btn_resp.context_menu(|ui| {
                        if ui.button("Delete from library").clicked() {
                            delete = Some(e.path.clone());
                            ui.close_menu();
                        }
                    });
                }
            }

            if node_entries.is_empty() && macro_entries.is_empty() && !frame_matches {
                ui.colored_label(theme::TEXT_DIM, "No matches.");
            }
        });

        if let Some(p) = delete {
            self.confirm_delete_macro = Some(p);
        }
    }

    /// Render the consolidated Setup window body: a top tab bar plus the
    /// active tab's manager UI.
    pub(crate) fn show_setup_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for (tab, label) in SETUP_TABS {
                ui.selectable_value(&mut self.setup_active_tab, *tab, *label);
            }
        });
        ui.separator();
        let mut rescan = false;
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            match self.setup_active_tab {
                SetupTab::Interfaces => {
                    let refs: Vec<u32> = self.object_manager.objects.iter().map(|o| o.interface_id).collect();
                    self.interface_manager.show(ui, &refs);
                }
                SetupTab::FixtureTemplates => {
                    let refs: Vec<u32> = self.object_manager.objects.iter().map(|o| o.fixture_id).collect();
                    self.fixture_manager.show(ui, &refs);
                    // Objects own a copy of their template's channels, so template edits must be pushed down.
                    let fixtures = &self.fixture_manager.fixtures;
                    for obj in &mut self.object_manager.objects {
                        if let Some(f) = fixtures.iter().find(|f| f.id == obj.fixture_id)
                            && !obj.matches_fixture(f)
                        {
                            obj.sync_with_fixture(f);
                            self.object_manager.needs_sync = true;
                        }
                    }
                }
                SetupTab::Objects => {
                    let interface_names: Vec<(u32, String)> = self.interface_manager.interfaces
                        .iter().map(|e| (e.id, e.name.clone())).collect();
                    let group_refs: Vec<u32> = self.group_manager.groups.iter().flat_map(|g| g.object_ids.iter().copied()).collect();
                    self.object_manager.show(ui, &self.fixture_manager.fixtures, &interface_names, &group_refs);
                }
                SetupTab::Groups => {
                    self.group_manager.show(ui, &self.object_manager.objects);
                }
                SetupTab::ColorPalettes => {
                    let refs: Vec<u32> = self.color_palette_group_manager.groups.iter().flat_map(|g| g.palette_ids.iter().copied()).collect();
                    self.color_palette_manager.show(ui, &refs);
                }
                SetupTab::ColorPaletteGroups => {
                    self.color_palette_group_manager.show(ui, &self.color_palette_manager.palettes);
                }
                SetupTab::Gradients => self.gradient_preset_manager.show(ui),
                SetupTab::AudioInputs => {
                    let missing = self.missing_hardware();
                    rescan |= widgets::audio_input_list::show(ui, &mut self.audio_inputs, &missing);
                }
                SetupTab::InputControllers => {
                    let missing = self.missing_hardware();
                    rescan |= widgets::input_controller_list::show(ui, &mut self.input_controllers, &missing);
                }
                SetupTab::DmxInputs => widgets::dmx_input_list::show(ui, &self.dmx_inputs),
            }
        });
        if rescan {
            self.rescan_hardware();
        }
    }

    /// Everything set up but not present: controllers, audio inputs, MIDI devices.
    pub(crate) fn missing_hardware(&self) -> Vec<String> {
        let mut missing = self.input_controllers.missing_controllers();
        missing.extend(self.audio_inputs.missing_inputs());
        missing.extend(self.midi_hub.missing_devices());
        missing
    }

    /// The one Rescan: the status bar and both hardware tabs call this.
    pub(crate) fn rescan_hardware(&mut self) {
        self.input_controllers.rescan();
        self.audio_inputs.force_rescan();
        self.midi_hub.rescan();
    }
}

/// Several frames selected: only what makes sense for all of them, the colour.
pub(crate) fn show_frames_colour_inspector(
    ui: &mut egui::Ui,
    graph: &mut widgets::nodes::NodeGraph,
    frame_ids: &[u64],
) {
    ui.heading(format!("{} frames selected", frame_ids.len()));
    ui.separator();
    let frames = graph.frames_mut();
    let Some(first_color) = frames.iter().find(|f| frame_ids.contains(&f.id)).map(|f| f.color) else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label("Color");
        let mut rgb = [
            first_color.r() as f32 / 255.0,
            first_color.g() as f32 / 255.0,
            first_color.b() as f32 / 255.0,
        ];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            let color = egui::Color32::from_rgb(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
            for frame in frames.iter_mut().filter(|f| frame_ids.contains(&f.id)) {
                frame.color = color;
            }
        }
    });
    ui.add_space(4.0);
    ui.colored_label(theme::TEXT_DIM, "Select one frame to edit its title and notes.");
}

/// Inline inspector for a selected decorative frame.
pub(crate) fn show_frame_inspector(
    ui: &mut egui::Ui,
    graph: &mut widgets::nodes::NodeGraph,
    frame_id: u64,
) {
    ui.heading("Frame");
    ui.separator();
    let frames = graph.frames_mut();
    let Some(frame) = frames.iter_mut().find(|f| f.id == frame_id) else {
        ui.label("Frame not found.");
        return;
    };
    ui.horizontal(|ui| {
        ui.label("Title");
        ui.add(egui::TextEdit::singleline(&mut frame.title).desired_width(160.0));
    });
    ui.horizontal(|ui| {
        ui.label("Color");
        let mut rgb = [
            frame.color.r() as f32 / 255.0,
            frame.color.g() as f32 / 255.0,
            frame.color.b() as f32 / 255.0,
        ];
        if ui.color_edit_button_rgb(&mut rgb).changed() {
            frame.color = egui::Color32::from_rgb(
                (rgb[0] * 255.0) as u8,
                (rgb[1] * 255.0) as u8,
                (rgb[2] * 255.0) as u8,
            );
        }
    });
    ui.label("Notes");
    ui.add(
        egui::TextEdit::multiline(&mut frame.notes)
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(8.0);
    ui.colored_label(
        theme::TEXT_DIM,
        "Drag the title bar to move; the bottom-right corner to resize. Delete key removes the frame.",
    );
}
