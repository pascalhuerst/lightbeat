use crate::LightBeatApp;
use crate::setup;
use crate::project;
use crate::widgets;
use std::path::PathBuf;
use eframe::egui;
use crate::engine::types::{EngineCommand, NodeId, SubgraphInnerCmd};
use crate::widgets::nodes::NodeGraph;

/// Result from a background file dialog.
/// What waits behind the "Unsaved changes" dialog.
#[derive(Debug, Clone)]
pub(crate) enum PendingAction {
    Quit,
    New,
    Open,
    OpenPath(PathBuf),
}

impl PendingAction {
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            PendingAction::Quit => "quitting",
            PendingAction::New => "starting a new project",
            PendingAction::Open | PendingAction::OpenPath(_) => "opening another project",
        }
    }
}

pub(crate) enum FileDialogResult {
    OpenProject(PathBuf),
    SaveProjectAs(PathBuf),
    ExportSetup(PathBuf),
    ImportSetup(PathBuf),
}

impl LightBeatApp {
    pub(crate) fn load_setup(&mut self) {
        match setup::load_setup() {
            Ok(s) => {
                // Direct assignment here; engine/context syncs happen in `new`
                // after this is called (avoids syncing before engine is ready).
                self.fixture_manager = widgets::fixture_list::FixtureManager::from_fixtures(s.fixtures);
                self.object_manager = widgets::object_list::ObjectManager::from_objects(s.objects);
                self.interface_manager = widgets::interface_list::InterfaceManager::from_saved(s.interfaces);
                self.group_manager = widgets::group_list::GroupManager::from_groups(s.groups);
                self.color_palette_manager = widgets::color_palette_list::ColorPaletteManager::from_palettes(s.color_palettes);
                self.color_palette_group_manager = widgets::color_palette_group_list::ColorPaletteGroupManager::from_groups(s.color_palette_groups);
                self.gradient_preset_manager = widgets::gradient_preset_list::GradientPresetManager::from_presets(s.gradient_presets);
                self.input_controllers.set_controllers(&s.input_controllers);
                self.audio_inputs.set_inputs(&s.audio_inputs);
                self.dmx_inputs.set_inputs(&s.dmx_inputs);
                // Reset history so undo doesn't go back to an empty setup.
                self.setup_undoer = egui::util::undoer::Undoer::default();
            }
            Err(e) => {
                self.setup_save_blocked = true;
                let kept = match setup::set_aside_bad_setup() {
                    Ok(bad) => format!("The unreadable file was kept as {}.", bad.display()),
                    Err(re) => format!("It could not be moved aside either ({re}); it stays where it is."),
                };
                self.report_error(format!(
                    "The setup could not be loaded ({e}). {kept} Setup changes are not saved this session; Import Setup from a good file to start saving again."
                ));
            }
        }
    }

    pub(crate) fn save_setup(&mut self) {
        self.setup_changed_at = None;
        let setup = self.current_setup();
        if let Err(e) = setup::save_setup(&setup, self.setup_save_blocked) {
            self.report_error(format!("Could not save the setup: {e}"));
        }
    }

    /// Show a file problem until the user dismisses it, and keep the terminal line.
    pub(crate) fn report_error(&mut self, message: String) {
        eprintln!("{message}");
        if !self.notices.contains(&message) {
            self.notices.push(message);
        }
    }

    /// Write the project to `path` and make it the current file. False on failure.
    fn write_project_to(&mut self, path: PathBuf) -> bool {
        if let Err(e) = project::save_to_file(&self.graph, self.view_state(), &path) {
            self.report_error(format!("Could not save the project to {}: {e}", path.display()));
            return false;
        }
        self.project_path = Some(path.clone());
        self.mark_project_saved();
        self.config.push_recent_project(&path);
        self.config.save();
        self.delete_autosave_for(Some(&path));
        true
    }

    /// File > Save and Ctrl+S. A project without a file asks for one (Save As);
    /// `then` runs once the project is on disk.
    pub(crate) fn save_command(&mut self, ctx: &egui::Context, then: Option<PendingAction>) {
        self.save_setup();
        match self.project_path.clone() {
            Some(path) => {
                if self.write_project_to(path)
                    && let Some(action) = then
                {
                    self.run_action(ctx, action);
                }
            }
            None => {
                if self.file_dialog_rx.is_none() {
                    self.after_save_as = then;
                }
                self.save_project_as(ctx);
            }
        }
    }

    /// Run `action` now if nothing would be lost, otherwise ask first.
    pub(crate) fn request_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        if self.is_project_dirty() {
            self.unsaved_prompt = Some(action);
        } else {
            self.run_action(ctx, action);
        }
    }

    pub(crate) fn run_action(&mut self, ctx: &egui::Context, action: PendingAction) {
        match action {
            PendingAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            PendingAction::New => self.new_project(),
            PendingAction::Open => self.open_project(ctx),
            PendingAction::OpenPath(p) => self.load_project_from(&p),
        }
    }

    /// Serialize the current project graph + view to JSON for dirty
    /// comparison and autosave writes.
    fn project_json(&self) -> String {
        let mut project = project::save_graph(&self.graph);
        project.view = Some(self.view_state());
        serde_json::to_string(&project).unwrap_or_default()
    }

    /// Call after a successful project save / load / apply to baseline the
    /// dirty detector against the now-on-disk state.
    pub(crate) fn mark_project_saved(&mut self) {
        self.last_saved_project_json = Some(self.project_json());
    }

    /// True when the current project differs from the last saved/loaded
    /// baseline. Returns true if there's no baseline yet (fresh graph —
    /// asks the user to save before losing their work).
    pub(crate) fn is_project_dirty(&self) -> bool {
        match &self.last_saved_project_json {
            Some(base) => self.project_json() != *base,
            None => {
                // No baseline: treat an empty graph as clean so launching the
                // app and quitting immediately doesn't prompt.
                !self.graph.root_level().nodes.is_empty()
            }
        }
    }

    /// Write the current project to its crash-recovery autosave file. Silent
    /// best-effort — errors are logged and ignored.
    pub(crate) fn write_autosave(&mut self) {
        let path = project::autosave_path_for(self.project_path.as_deref());
        let mut project = project::save_graph(&self.graph);
        project.view = Some(self.view_state());
        match serde_json::to_string_pretty(&project) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    self.report_error(format!("Autosave failed ({}): {e}", path.display()));
                }
            }
            Err(e) => self.report_error(format!("Autosave failed: {e}")),
        }
    }

    /// Remove the autosave file associated with the given project path (or
    /// the unsaved-project default if `None`). Missing-file is not an error.
    pub(crate) fn delete_autosave_for(&self, project_path: Option<&std::path::Path>) {
        let path = project::autosave_path_for(project_path);
        let _ = std::fs::remove_file(&path);
    }

    fn view_state(&self) -> project::ProjectViewState {
        // Panel visibility lives in settings.json now; the field is only kept
        // (at its old default) because the project format still has it.
        project::ProjectViewState { show_library: true }
    }

    pub(crate) fn save_project_as(&mut self, ctx: &egui::Context) {
        if self.file_dialog_rx.is_some() { return; } // dialog already open
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_dialog_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new()
                .set_title("Save Project As")
                .add_filter("LightBeat Project", &["json"])
                .set_file_name("project.json");
            if let Some(path) = dialog.save_file() {
                let _ = tx.send(FileDialogResult::SaveProjectAs(path));
            }
            ctx.request_repaint();
        });
    }

    fn open_project(&mut self, ctx: &egui::Context) {
        if self.file_dialog_rx.is_some() { return; } // dialog already open
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_dialog_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new()
                .set_title("Open Project")
                .add_filter("LightBeat Project", &["json"]);
            if let Some(path) = dialog.pick_file() {
                let _ = tx.send(FileDialogResult::OpenProject(path));
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn export_setup_dialog(&mut self, ctx: &egui::Context) {
        if self.file_dialog_rx.is_some() { return; }
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_dialog_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new()
                .set_title("Export Setup")
                .add_filter("LightBeat Setup", &["json"])
                .set_file_name("lightbeat-setup.json");
            if let Some(path) = dialog.save_file() {
                let _ = tx.send(FileDialogResult::ExportSetup(path));
            }
            ctx.request_repaint();
        });
    }

    pub(crate) fn import_setup_dialog(&mut self, ctx: &egui::Context) {
        if self.file_dialog_rx.is_some() { return; }
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_dialog_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new()
                .set_title("Import Setup")
                .add_filter("LightBeat Setup", &["json"]);
            if let Some(path) = dialog.pick_file() {
                let _ = tx.send(FileDialogResult::ImportSetup(path));
            }
            ctx.request_repaint();
        });
    }

    /// Replace the setup with one from a file, adapted to this computer. The
    /// old setup stays reachable with undo.
    fn import_setup(&mut self, path: &std::path::Path) {
        let mut imported = match setup::import_from(path) {
            Ok(s) => s,
            Err(e) => {
                self.setup_notice = Some(vec![format!("Could not import {}: {e}", path.display())]);
                return;
            }
        };
        let notes = setup::adapt_to_machine(&mut imported, &setup::Machine::probe());
        self.apply_setup(&imported);
        // The unreadable file was set aside at start, so this is the first
        // good setup there is again.
        self.setup_save_blocked = false;
        self.save_setup();
        self.setup_notice = Some(if notes.is_empty() {
            vec!["Setup imported. Everything it refers to was found on this computer.".to_string()]
        } else {
            std::iter::once("Setup imported. Some things it refers to are different on this computer:".to_string())
                .chain(notes)
                .collect()
        });
    }

    pub(crate) fn poll_file_dialog(&mut self, ctx: &egui::Context) {
        let rx = match &self.file_dialog_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(result) => {
                self.file_dialog_rx = None;
                match result {
                    FileDialogResult::OpenProject(path) => {
                        self.load_project_from(&path);
                    }
                    FileDialogResult::ExportSetup(path) => {
                        let message = match setup::export_to(&self.current_setup(), &path) {
                            Ok(()) => format!("Setup exported to {}.", path.display()),
                            Err(e) => format!("Could not export the setup: {e}"),
                        };
                        self.setup_notice = Some(vec![message]);
                    }
                    FileDialogResult::ImportSetup(path) => self.import_setup(&path),
                    FileDialogResult::SaveProjectAs(path) => {
                        let then = self.after_save_as.take();
                        if self.write_project_to(path)
                            && let Some(action) = then
                        {
                            self.run_action(ctx, action);
                        }
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Dialog was cancelled (thread finished without sending a result).
                self.file_dialog_rx = None;
                self.after_save_as = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Dialog still open, keep waiting.
            }
        }
    }

    pub(crate) fn send_load_data_recursive(
        &mut self,
        saved_nodes: &[project::SavedNode],
        indices: &[usize],
        subgraph_path: Vec<NodeId>,
    ) {
        for (i, saved) in saved_nodes.iter().enumerate() {
            if i >= indices.len() { continue; }
            let node_id = NodeId(saved.id);

            if let Some(data) = &saved.data {
                if subgraph_path.is_empty() {
                    self.engine.send(EngineCommand::LoadData {
                        node_id,
                        data: data.clone(),
                    });
                } else {
                    self.engine.send(EngineCommand::SubgraphInnerCommand {
                        subgraph_path: subgraph_path.clone(),
                        command: Box::new(SubgraphInnerCmd::LoadData {
                            node_id,
                            data: data.clone(),
                        }),
                    });
                }
            }

            // Recurse into inner graph.
            if saved.type_name == "Subgraph"
                && let Some(inner_project) = &saved.inner_graph {
                    let mut inner_path = subgraph_path.clone();
                    inner_path.push(node_id);

                    // Inner indices: the inner graph was loaded starting at index 2
                    // (after bridge nodes at 0 and 1).
                    let inner_indices: Vec<usize> = (0..inner_project.nodes.len())
                        .map(|j| j + 2) // offset by bridge nodes
                        .collect();

                    self.send_load_data_recursive(
                        &inner_project.nodes,
                        &inner_indices,
                        inner_path,
                    );
                }
        }
    }

    /// Apply an in-memory ProjectFile, replacing the current graph & engine state.
    /// Used both for file-load and for undo/redo. Preserves pan/zoom and the
    /// active subgraph path across the rebuild.
    pub(crate) fn apply_project(&mut self, proj: &project::ProjectFile) {
        let view = self.graph.capture_view_state();

        self.engine.send(EngineCommand::RemoveAllNodes);
        self.graph = NodeGraph::new();
        self.register_node_factories();

        let indices = project::load_graph(&mut self.graph, proj);
        self.wire_new_nodes();
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }
        self.send_load_data_recursive(&proj.nodes, &indices, vec![]);

        self.graph.restore_view_state(&view);
    }

    pub(crate) fn load_project_from(&mut self, path: &std::path::Path) {
        match project::load_from_file(&path.to_path_buf()) {
            Ok(proj) => {
                self.apply_project(&proj);
                self.graph.fit_to_content();
                self.project_path = Some(path.to_path_buf());
                // Reset history so undo doesn't go back to the empty pre-load graph.
                self.project_undoer = egui::util::undoer::Undoer::default();
                self.mark_project_saved();
                self.config.push_recent_project(path);
                self.config.save();
                // If a crash-recovery autosave exists for this project and
                // is newer than the project file on disk, surface a prompt
                // so the user can choose to recover.
                let autosave = project::autosave_path_for(Some(path));
                if autosave.exists() && is_newer(&autosave, path) {
                    self.recover_prompt = Some(autosave);
                }
            }
            Err(e) => {
                self.report_error(format!("Could not open {}: {e}", path.display()));
            }
        }
    }

    /// Clear the graph to an empty project and forget the current file path.
    /// The Setup (fixtures, interfaces, palettes, etc.) is left alone —
    /// that's a separate file and the user may want to keep working with
    /// the same hardware set-up across projects.
    fn new_project(&mut self) {
        let empty = project::ProjectFile {
            version: project::PROJECT_VERSION,
            nodes: Vec::new(),
            connections: Vec::new(),
            frames: Vec::new(),
            view: None,
            bridge_in_pos: None,
            bridge_out_pos: None,
        };
        self.apply_project(&empty);
        self.project_path = None;
        // Drop undo history so the first undo doesn't resurrect the previous
        // project's graph.
        self.project_undoer = egui::util::undoer::Undoer::default();
        // Baseline the dirty detector against the empty graph — a freshly
        // created project isn't "unsaved" until the user actually edits it.
        self.mark_project_saved();
    }

    /// Build current SetupFile snapshot (used for both saving and feeding the undoer).
    pub(crate) fn current_setup(&self) -> setup::SetupFile {
        setup::SetupFile {
            fixtures: self.fixture_manager.fixtures.clone(),
            objects: self.object_manager.objects.clone(),
            interfaces: self.interface_manager.to_saved(),
            groups: self.group_manager.groups.clone(),
            color_palettes: self.color_palette_manager.palettes.clone(),
            color_palette_groups: self.color_palette_group_manager.groups.clone(),
            gradient_presets: self.gradient_preset_manager.presets.clone(),
            input_controllers: self.input_controllers.export(),
            audio_inputs: self.audio_inputs.export(),
            dmx_inputs: self.dmx_inputs.export(),
        }
    }

    /// Apply an in-memory SetupFile, rebuilding all managers and re-syncing engine state.
    pub(crate) fn apply_setup(&mut self, s: &setup::SetupFile) {
        self.fixture_manager = widgets::fixture_list::FixtureManager::from_fixtures(s.fixtures.clone());
        self.object_manager = widgets::object_list::ObjectManager::from_objects(s.objects.clone());
        self.interface_manager = widgets::interface_list::InterfaceManager::from_saved(s.interfaces.clone());
        self.group_manager = widgets::group_list::GroupManager::from_groups(s.groups.clone());
        self.color_palette_manager = widgets::color_palette_list::ColorPaletteManager::from_palettes(s.color_palettes.clone());
        self.color_palette_group_manager = widgets::color_palette_group_list::ColorPaletteGroupManager::from_groups(s.color_palette_groups.clone());
        self.gradient_preset_manager = widgets::gradient_preset_list::GradientPresetManager::from_presets(s.gradient_presets.clone());
        self.input_controllers.set_controllers(&s.input_controllers);
        self.audio_inputs.set_inputs(&s.audio_inputs);
        self.dmx_inputs.set_inputs(&s.dmx_inputs);

        self.sync_group_context();
        self.sync_object_store();
        self.sync_interfaces();
        self.sync_palette_context();
        self.sync_gradient_library();
    }
}

/// Inline inspector for a selected decorative frame.
/// True when `a`'s modification time is strictly newer than `b`'s. Missing
/// metadata on either side yields `false` — we don't want to false-positive
/// into prompting the user when we can't actually compare.
fn is_newer(a: &std::path::Path, b: &std::path::Path) -> bool {
    let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
    let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
    match (ta, tb) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}
