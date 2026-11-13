mod beat_bench;
mod audio;
mod beat_clock;
mod color;
mod config;
mod dmx_input;
mod dmx_io;
mod engine;
mod input_controller;
mod macros;
mod midi_io;
mod setup;
mod interfaces;
mod latency;
mod link_controller;
mod objects;
mod paths;
mod perf;
mod project;
mod theme;
mod update;
mod version;
mod widgets;
mod app;
use app::macros::SaveMacroDialog;
use app::files::{PendingAction, FileDialogResult};
use app::panels::SetupTab;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};
use audio::manager::AudioInputManager;
use config::AppConfig;
use macros::library::LibraryManager;
use engine::types::{new_shared_state, EngineCommand, NodeId};
use engine::EngineHandle;
use input_controller::InputControllerManager;
use engine::nodes::io::clock::ClockProcessNode;
use widgets::nodes::io::clock::ClockWidget;
use widgets::nodes::math::palette_select::new_shared_palette_context;
use widgets::nodes::NodeGraph;

/// Undo/redo requested from the Edit menu. The menu closure holds `&mut self`,
/// so the action is recorded and carried out after the panel closes, where a
/// fresh project/setup snapshot can be built without fighting the borrow.
#[derive(Clone, Copy)]
enum UndoAction {
    UndoProject,
    RedoProject,
    UndoSetup,
    RedoSetup,
}

struct LightBeatApp {
    graph: NodeGraph,
    engine: EngineHandle,
    perf: perf::SharedPerfStats,
    ui_timers: perf::UiTimers,
    /// Context time of the last input event that could have mutated the
    /// project or setup. Undo snapshots are gated on this — see the snapshot
    /// block in `update`.
    last_interaction_time: f64,
    /// Context time of the last undo snapshot, used to cap snapshot rate
    /// while the interaction window is open.
    last_snapshot_time: f64,
    /// Cached Edit-menu enable states, refreshed whenever a snapshot is taken.
    can_undo_proj: bool,
    can_redo_proj: bool,
    can_undo_setup: bool,
    can_redo_setup: bool,
    pending_undo_action: Option<UndoAction>,
    /// The document that changed last; Undo and Redo act on it.
    last_changed_doc: Option<app::undo::UndoDoc>,
    /// The project as last seen by the change detector, to tell which of the two
    /// documents an edit belongs to.
    project_baseline: Option<project::ProjectFile>,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
    config: AppConfig,
    project_path: Option<PathBuf>,
    file_dialog_rx: Option<std::sync::mpsc::Receiver<FileDialogResult>>,
    /// What an imported setup could not match on this computer, shown until dismissed.
    setup_notice: Option<Vec<String>>,
    snapshot: Arc<parking_lot::Mutex<beat_clock::LinkSnapshot>>,
    quit_requested: bool,
    updater: update::Updater,
    last_update_check: Option<std::time::Instant>,
    show_update_window: bool,
    show_about: bool,
    show_shortcuts: bool,
    show_preferences: bool,
    /// Serialized form (JSON) of the project at the moment it was last saved
    /// (or loaded). Comparing the current serialized project against this
    /// tells us whether there are unsaved changes.
    last_saved_project_json: Option<String>,
    /// Time of the last autosave write, in seconds from app start (ctx time).
    last_autosave_time: f64,
    /// The action the "Unsaved changes" dialog is open for, if it is open.
    unsaved_prompt: Option<PendingAction>,
    /// Action to run once the Save As dialog has written the project.
    after_save_as: Option<PendingAction>,
    /// File errors (project, setup, macros, autosave) shown until dismissed.
    notices: Vec<String>,
    engine_stop_reported: bool,
    /// A failed settings write waits here for `report_settings_failure`.
    settings_save_error: Option<String>,
    settings_failure_reported: bool,
    /// Set when the setup file existed but could not be read: every setup save
    /// is refused for the session so the empty setup never replaces it.
    setup_save_blocked: bool,
    /// The setup as last seen by the change detector, and when it last changed
    /// (for the debounced setup save).
    setup_baseline: setup::SetupFile,
    setup_changed_at: Option<f64>,
    /// Macro file waiting for the user to confirm its deletion.
    confirm_delete_macro: Option<PathBuf>,
    last_window_title: String,
    /// Set once the user chose Discard, so the close request that follows is
    /// not intercepted again while the project is still dirty.
    close_discarded: bool,
    /// Whether the startup autosave-recovery prompt is currently open. Carries
    /// the autosave path to load on "Recover".
    recover_prompt: Option<PathBuf>,
    show_dmx_monitor: bool,
    show_setup: bool,
    setup_active_tab: SetupTab,
    dmx_monitor: widgets::dmx_monitor::DmxMonitor,
    dmx_shared: dmx_io::SharedDmxState,
    object_store: dmx_io::SharedObjectStore,
    effect_hub: engine::effects::EffectHub,
    stack_names: widgets::nodes::output::effect::SharedStackNames,
    group_ctx: widgets::group_context::SharedGroupContext,
    fixture_manager: widgets::fixture_list::FixtureManager,
    object_manager: widgets::object_list::ObjectManager,
    interface_manager: widgets::interface_list::InterfaceManager,
    group_manager: widgets::group_list::GroupManager,
    color_palette_manager: widgets::color_palette_list::ColorPaletteManager,
    color_palette_group_manager: widgets::color_palette_group_list::ColorPaletteGroupManager,
    palette_ctx: widgets::nodes::math::palette_select::SharedPaletteContext,
    gradient_preset_manager: widgets::gradient_preset_list::GradientPresetManager,
    gradient_library: widgets::nodes::math::gradient_source::SharedGradientLibrary,
    input_controllers: InputControllerManager,
    /// MIDI devices of the MIDI Input and MIDI Output nodes.
    midi_hub: midi_io::MidiHub,
    /// What the MIDI Input nodes have learned, for the MIDI Output nodes to copy.
    midi_learned: widgets::nodes::io::midi_common::SharedLearned,
    audio_inputs: AudioInputManager,
    /// DMX from other lighting desks (Art-Net).
    dmx_inputs: dmx_input::DmxInputHub,
    /// Portal Sends publish here by name; Portal Receives read.
    portals: engine::nodes::meta::portal::SharedPortalRegistry,
    library: LibraryManager,
    show_add_nodes: bool,
    library_search: String,
    save_macro_dialog: Option<SaveMacroDialog>,
    project_undoer: egui::util::undoer::Undoer<project::ProjectFile>,
    setup_undoer: egui::util::undoer::Undoer<setup::SetupFile>,
}

impl LightBeatApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        update::clean_up_leftovers();
        // Add Phosphor icons as a fallback font so we can use icon glyphs
        // (constants like egui_phosphor::regular::ARROW_UP) anywhere in the UI.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let config = AppConfig::load();
        latency::set_lead_ms(config.latency_ms);
        if config.msaa_samples > 0 {
            cc.egui_ctx.tessellation_options_mut(|o| o.feathering = false);
        }
        let (open_add_nodes, open_setup, open_monitor, setup_tab) =
            (config.show_add_nodes, config.show_setup, config.show_dmx_monitor, config.setup_tab);
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();
        let dmx_shared = dmx_io::new_shared_dmx_state();
        // Apply the persisted "start with DMX bypassed" preference before
        // the engine starts ticking. Default is live; users who want the
        // app to wake up silent set `dmx_bypass_on_startup: true` in
        // settings.json.
        if config.dmx_bypass_on_startup {
            dmx_shared.lock().bypass = true;
        }
        let object_store = dmx_io::new_shared_object_store();
        let group_ctx = widgets::group_context::new_shared_group_context();
        let perf = perf::new_shared_perf_stats();
        let effect_hub = engine::effects::EffectHub::new();
        let stack_names = widgets::nodes::output::effect::new_shared_stack_names();
        let engine = EngineHandle::start(dmx_shared.clone(), object_store.clone(), effect_hub.clone(), perf.clone());

        let graph = NodeGraph::new();
        let mut app = Self {
            graph,
            effect_hub,
            stack_names,
            engine,
            perf,
            ui_timers: perf::UiTimers::default(),
            last_interaction_time: 0.0,
            last_snapshot_time: 0.0,
            can_undo_proj: false,
            can_redo_proj: false,
            can_undo_setup: false,
            can_redo_setup: false,
            pending_undo_action: None,
            last_changed_doc: None,
            project_baseline: None,
            beat_clock,
            _subs: Vec::new(),
            config,
            project_path: None,
            file_dialog_rx: None,
            setup_notice: None,
            snapshot,
            quit_requested: false,
            updater: update::Updater::default(),
            last_update_check: None,
            show_update_window: false,
            show_about: false,
            show_shortcuts: false,
            show_preferences: false,
            last_saved_project_json: None,
            last_autosave_time: 0.0,
            unsaved_prompt: None,
            after_save_as: None,
            notices: Vec::new(),
            engine_stop_reported: false,
            settings_save_error: None,
            settings_failure_reported: false,
            setup_save_blocked: false,
            setup_baseline: setup::SetupFile::default(),
            setup_changed_at: None,
            confirm_delete_macro: None,
            last_window_title: String::new(),
            close_discarded: false,
            recover_prompt: None,
            show_dmx_monitor: open_monitor,
            show_setup: open_setup,
            setup_active_tab: setup_tab,
            dmx_monitor: widgets::dmx_monitor::DmxMonitor::new(),
            dmx_shared,
            object_store,
            group_ctx,
            fixture_manager: widgets::fixture_list::FixtureManager::new(),
            object_manager: widgets::object_list::ObjectManager::new(),
            interface_manager: widgets::interface_list::InterfaceManager::new(),
            group_manager: widgets::group_list::GroupManager::new(),
            color_palette_manager: widgets::color_palette_list::ColorPaletteManager::new(),
            color_palette_group_manager: widgets::color_palette_group_list::ColorPaletteGroupManager::new(),
            palette_ctx: new_shared_palette_context(),
            gradient_preset_manager: widgets::gradient_preset_list::GradientPresetManager::new(),
            gradient_library: widgets::nodes::math::gradient_source::new_shared_gradient_library(),
            input_controllers: InputControllerManager::new(),
            midi_hub: midi_io::MidiHub::start(),
            midi_learned: widgets::nodes::io::midi_common::new_shared_learned(),
            audio_inputs: AudioInputManager::new(),
            dmx_inputs: dmx_input::DmxInputHub::start(),
            portals: std::sync::Arc::new(parking_lot::Mutex::new(
                engine::nodes::meta::portal::PortalRegistry::default(),
            )),
            library: LibraryManager::new(macros::default_library_root()),
            show_add_nodes: open_add_nodes,
            library_search: String::new(),
            save_macro_dialog: None,
            project_undoer: egui::util::undoer::Undoer::default(),
            setup_undoer: egui::util::undoer::Undoer::default(),
        };

        // Load hardware setup (fixtures + interfaces).
        app.load_setup();
        app.setup_baseline = app.current_setup();

        app.register_node_factories();
        app.sync_group_context();
        app.sync_object_store();
        app.sync_interfaces();
        app.sync_palette_context();
        app.sync_gradient_library();

        // Try autoload: prefer the most-recent-opened project, falling back
        // to the default path next to the executable.
        let loaded = if app.config.autoload_on_open {
            let candidate = app.config.recent_projects.first().cloned()
                .filter(|p| p.exists())
                .or_else(|| {
                    let p = project::default_project_path();
                    if p.exists() { Some(p) } else { None }
                });
            if let Some(path) = candidate {
                app.load_project_from(&path);
                true
            } else {
                false
            }
        } else {
            false
        };

        if !loaded {
            app.create_default_clock();
            // Drain since create_default_clock already handled the engine side.
            app.graph.drain_new_nodes();
            // Baseline the dirty detector so a fresh default-clock graph
            // isn't immediately flagged as "unsaved changes".
            app.mark_project_saved();
        }

        app
    }

    /// Sync group and object data to the shared context the effect stacks read.
    fn sync_group_context(&self) {
        let mut ctx = self.group_ctx.lock();
        ctx.groups = self.group_manager.groups.clone();
        ctx.objects = self.object_manager.objects.clone();
    }

    fn sync_palette_context(&self) {
        let mut ctx = self.palette_ctx.lock();
        ctx.palettes = self.color_palette_manager.palettes.clone();
        ctx.groups = self.color_palette_group_manager.groups.clone();
        ctx.version += 1;
    }

    fn sync_gradient_library(&self) {
        let mut lib = self.gradient_library.lock();
        lib.presets = self.gradient_preset_manager.presets.clone();
    }

    /// Move any "Save current as preset" requests pushed by Gradient Source
    /// widgets into the manager, assigning fresh ids, then re-sync the mirror
    /// so all Gradient Sources see the new preset in their dropdowns.
    fn drain_pending_gradient_saves(&mut self) {
        let (pending, updates) = {
            let mut lib = self.gradient_library.lock();
            (std::mem::take(&mut lib.pending_saves), std::mem::take(&mut lib.pending_updates))
        };
        if pending.is_empty() && updates.is_empty() { return; }
        for up in updates {
            if let Some(p) = self.gradient_preset_manager.presets.iter_mut().find(|p| p.id == up.id) {
                p.stops = up.stops;
            }
        }
        for req in pending {
            let id = self.gradient_preset_manager.next_id();
            self.gradient_preset_manager.presets.push(
                crate::objects::gradient_preset::GradientPreset {
                    id,
                    name: req.name,
                    stops: req.stops,
                },
            );
        }
        self.sync_gradient_library();
    }

    fn create_default_clock(&mut self) {
        let id = NodeId(1);
        let shared = new_shared_state(0, 3);
        let engine_node = ClockProcessNode::new(id, Arc::clone(&self.snapshot));
        let beat_state = engine_node.beat_state.clone();
        let widget = ClockWidget::new(id, Arc::clone(&shared));

        self.graph.add_node(Box::new(widget), egui::pos2(50.0, 50.0));
        self.engine.send(EngineCommand::AddNode {
            node: Box::new(engine_node),
            shared,
        });
        self._subs.push(
            self.beat_clock
                .subscribe(BeatPattern::every(1), beat_state),
        );
    }

    /// Sync object instances from the UI manager to the engine's shared store.
    fn sync_object_store(&self) {
        let mut store = self.object_store.lock();
        store.objects = self.object_manager.objects.clone();
    }

    /// Build and send DMX output interfaces to the engine.
    fn sync_interfaces(&mut self) {
        let mut ifaces: Vec<(u32, Box<dyn interfaces::DmxOutput>)> = Vec::new();
        for entry in &self.interface_manager.interfaces {
            if !entry.enabled { continue; }
            let config = interfaces::DmxOutputConfig::from_output_config(&entry.config);
            if let Some(cfg) = config {
                match cfg.build() {
                    Ok(output) => ifaces.push((entry.id, output)),
                    Err(e) => eprintln!("Failed to create interface '{}': {}", entry.name, e),
                }
            }
        }
        self.engine.send(EngineCommand::SetInterfaces(ifaces));
    }

    /// The undo or redo the Edit menu and the shortcut stand for right now.
    pub(crate) fn undo_action(&self, redo: bool) -> Option<UndoAction> {
        let (project, setup) = if redo {
            (self.can_redo_proj, self.can_redo_setup)
        } else {
            (self.can_undo_proj, self.can_undo_setup)
        };
        app::undo::pick_target(self.last_changed_doc, project, setup).map(|doc| match (doc, redo) {
            (app::undo::UndoDoc::Project, false) => UndoAction::UndoProject,
            (app::undo::UndoDoc::Project, true) => UndoAction::RedoProject,
            (app::undo::UndoDoc::Setup, false) => UndoAction::UndoSetup,
            (app::undo::UndoDoc::Setup, true) => UndoAction::RedoSetup,
        })
    }

    /// Apply an undo or redo. Builds the comparison snapshot on demand: the
    /// per-frame snapshot is gated on recent input and may be a frame or two
    /// stale, and these only run on a keypress or menu click.
    fn run_undo_action(&mut self, action: UndoAction) {
        match action {
            UndoAction::UndoProject => {
                let snap = project::save_graph(&self.graph);
                if let Some(p) = self.project_undoer.undo(&snap).cloned() {
                    self.apply_project(&p);
                }
            }
            UndoAction::RedoProject => {
                let snap = project::save_graph(&self.graph);
                if let Some(p) = self.project_undoer.redo(&snap).cloned() {
                    self.apply_project(&p);
                }
            }
            UndoAction::UndoSetup => {
                let snap = self.current_setup();
                if let Some(s) = self.setup_undoer.undo(&snap).cloned() {
                    self.apply_setup(&s);
                }
            }
            UndoAction::RedoSetup => {
                let snap = self.current_setup();
                if let Some(s) = self.setup_undoer.redo(&snap).cloned() {
                    self.apply_setup(&s);
                }
            }
        }
    }

    // -- Macro library helpers ----------------------------------------------

    pub(crate) fn window_title(&self) -> String {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled");
        format!("LightBeat - {}", name)
    }
}

impl LightBeatApp {
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(secs) = frame.info().cpu_usage {
            self.ui_timers.record_eframe(&self.perf, (secs * 1_000_000.0) as u64);
        }
        let started = std::time::Instant::now();
        self.update_frame(ctx);
        self.ui_timers
            .record_frame(&self.perf, started.elapsed().as_micros() as u64);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Setup (fixtures/interfaces/palettes/etc.) always persists on exit
        // — it's shared hardware config, not the user-authored project.
        self.save_setup();
        // Clean exit: remove the crash-recovery autosave file. If we got
        // here after a Discard or Save the user already made an explicit
        // decision; if the user closed without prompting (clean no-op),
        // there are no unsaved changes so the autosave is stale anyway.
        self.delete_autosave_for(self.project_path.as_deref());
        self.save_settings();
    }
}

fn main() -> eframe::Result {
    if beat_bench::run_if_requested() {
        return Ok(());
    }
    let saved = AppConfig::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(saved.window_size.unwrap_or((1280.0, 768.0)));
    if saved.window_maximized {
        viewport = viewport.with_maximized(true);
    }
    let options = eframe::NativeOptions {
        viewport,
        multisampling: saved.msaa_samples,
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
