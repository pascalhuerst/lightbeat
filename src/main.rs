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
use app::macros::{SaveMacroDialog, MacroDragPayload};
use app::files::{PendingAction, FileDialogResult};
use app::panels::{SetupTab, NodeDragPayload, FrameDragPayload, show_frame_inspector};

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};
use audio::manager::AudioInputManager;
use config::{AppConfig, InspectorMode};
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
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
    config: AppConfig,
    project_path: Option<PathBuf>,
    file_dialog_rx: Option<std::sync::mpsc::Receiver<FileDialogResult>>,
    /// What an imported setup could not match on this computer, shown until dismissed.
    setup_notice: Option<Vec<String>>,
    snapshot: Arc<std::sync::Mutex<beat_clock::LinkSnapshot>>,
    quit_requested: bool,
    updater: update::Updater,
    last_update_check: Option<std::time::Instant>,
    show_update_window: bool,
    show_about: bool,
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
    show_library: bool,
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
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();
        let dmx_shared = dmx_io::new_shared_dmx_state();
        // Apply the persisted "start with DMX bypassed" preference before
        // the engine starts ticking. Default is live; users who want the
        // app to wake up silent set `dmx_bypass_on_startup: true` in
        // settings.json.
        if config.dmx_bypass_on_startup {
            dmx_shared.lock().unwrap().bypass = true;
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
            last_saved_project_json: None,
            last_autosave_time: 0.0,
            unsaved_prompt: None,
            after_save_as: None,
            notices: Vec::new(),
            setup_save_blocked: false,
            setup_baseline: setup::SetupFile::default(),
            setup_changed_at: None,
            confirm_delete_macro: None,
            last_window_title: String::new(),
            close_discarded: false,
            recover_prompt: None,
            show_dmx_monitor: false,
            show_setup: false,
            setup_active_tab: SetupTab::Interfaces,
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
            portals: std::sync::Arc::new(std::sync::Mutex::new(
                engine::nodes::meta::portal::PortalRegistry::default(),
            )),
            library: LibraryManager::new(macros::default_library_root()),
            show_library: true,
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
        let mut ctx = self.group_ctx.lock().unwrap();
        ctx.groups = self.group_manager.groups.clone();
        ctx.objects = self.object_manager.objects.clone();
    }

    fn sync_palette_context(&self) {
        let mut ctx = self.palette_ctx.lock().unwrap();
        ctx.palettes = self.color_palette_manager.palettes.clone();
        ctx.groups = self.color_palette_group_manager.groups.clone();
        ctx.version += 1;
    }

    fn sync_gradient_library(&self) {
        let mut lib = self.gradient_library.lock().unwrap();
        lib.presets = self.gradient_preset_manager.presets.clone();
    }

    /// Move any "Save current as preset" requests pushed by Gradient Source
    /// widgets into the manager, assigning fresh ids, then re-sync the mirror
    /// so all Gradient Sources see the new preset in their dropdowns.
    fn drain_pending_gradient_saves(&mut self) {
        let pending: Vec<widgets::nodes::math::gradient_source::PendingPresetSave> = {
            let mut lib = self.gradient_library.lock().unwrap();
            std::mem::take(&mut lib.pending_saves)
        };
        if pending.is_empty() { return; }
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
        let mut store = self.object_store.lock().unwrap();
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

    fn window_title(&self) -> String {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled");
        format!("LightBeat - {}", name)
    }
}

/// How long after the last input the UI keeps repainting at full rate.
const ACTIVE_REPAINT_WINDOW: f64 = 2.0;
/// Margin before the intended idle frame time, so the wake-up precedes the vblank it targets.
const IDLE_WAKE_EARLY_SECS: f32 = 0.003;

impl LightBeatApp {
    fn update_frame(&mut self, ctx: &egui::Context) {
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

        let (can_undo_proj, can_redo_proj) = (self.can_undo_proj, self.can_redo_proj);
        let (can_undo_setup, can_redo_setup) = (self.can_undo_setup, self.can_redo_setup);

        if let Some(lines) = self.setup_notice.clone() {
            let mut open = true;
            let mut dismiss = false;
            egui::Window::new("Import / Export Setup").open(&mut open).collapsible(false).resizable(true).show(ctx, |ui| {
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
                    if ui.button("Open...").clicked() {
                        ui.close_menu();
                        self.request_action(ctx, PendingAction::Open);
                    }
                    let recents = self.config.recent_projects.clone();
                    let mut load_recent: Option<PathBuf> = None;
                    let mut clear_recents = false;
                    ui.add_enabled_ui(!recents.is_empty(), |ui| {
                        ui.menu_button("Open Recent", |ui| {
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
                        self.config.save();
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        ui.close_menu();
                        self.save_command(ctx, None);
                    }
                    if ui.button("Save As...").clicked() {
                        ui.close_menu();
                        self.save_project_as(ctx);
                    }
                    ui.separator();
                    if ui.button("Export Setup...").on_hover_text("Fixtures, objects, groups, interfaces, palettes, controllers and audio inputs, as one file").clicked() {
                        ui.close_menu();
                        self.export_setup_dialog(ctx);
                    }
                    if ui.button("Import Setup...").on_hover_text("Replace the setup with one from a file; Edit, Undo brings the old one back").clicked() {
                        ui.close_menu();
                        self.import_setup_dialog(ctx);
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        self.quit_requested = true;
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(can_undo_proj, egui::Button::new("Undo Project")).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = Some(UndoAction::UndoProject);
                    }
                    if ui.add_enabled(can_redo_proj, egui::Button::new("Redo Project")).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = Some(UndoAction::RedoProject);
                    }
                    ui.separator();
                    if ui.add_enabled(can_undo_setup, egui::Button::new("Undo Setup")).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = Some(UndoAction::UndoSetup);
                    }
                    if ui.add_enabled(can_redo_setup, egui::Button::new("Redo Setup")).clicked() {
                        ui.close_menu();
                        self.pending_undo_action = Some(UndoAction::RedoSetup);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_library, "Add Nodes").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_dmx_monitor, "DMX Monitor").changed() {
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
                        if changed { self.config.save(); }
                    });
                });
                ui.menu_button("Window", |ui| {
                    if ui.checkbox(&mut self.show_setup, "Setup…").changed() {
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button(format!("About LightBeat {}", version::version())).clicked() {
                        ui.close_menu();
                        self.show_about = true;
                    }
                    ui.separator();
                    if ui.button("Check for Updates").clicked() {
                        ui.close_menu();
                        self.show_update_window = true;
                        self.updater.check(self.config.update_channel);
                        self.last_update_check = Some(std::time::Instant::now());
                    }
                    let mut auto = self.config.check_for_updates;
                    if ui.checkbox(&mut auto, "Check automatically").changed() {
                        self.config.check_for_updates = auto;
                        self.config.save();
                    }
                    ui.menu_button(format!("Update channel: {}", self.config.update_channel.label()), |ui| {
                        for channel in update::Channel::ALL {
                            if ui.selectable_label(self.config.update_channel == channel, channel.label()).clicked() {
                                self.config.update_channel = channel;
                                self.config.save();
                                ui.close_menu();
                            }
                        }
                    });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(theme::TEXT_DIM, format!("LightBeat {}", version::version()));
                });
            });
        });

        self.update_check_tick();
        self.show_update_ui(ctx);

        // Consolidated Setup window — embedded `egui::Window` inside the
        // main viewport. All setup managers (interfaces, fixtures, objects,
        // groups, palettes, gradients, audio inputs, input controllers)
        // live as tabs inside one window.
        let mut setup_hovered = false;
        if self.show_setup {
            // Stage the open flag locally so the closure can borrow `self`
            // mutably; assign back after the window returns.
            let mut open = true;
            let r = egui::Window::new("Setup")
                .open(&mut open)
                .default_size([780.0, 560.0])
                .min_width(420.0)
                .min_height(320.0)
                .show(ctx, |ui| {
                    self.show_setup_tabs(ui);
                });
            self.show_setup = open;
            if let Some(ir) = r
                && ir.response.contains_pointer() { setup_hovered = true; }
        }

        self.input_controllers.reap_lost_sessions();

        // "Add Nodes" side panel (left) — unified node-registry + macro library.
        if self.show_library {
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
                        ui.colored_label(egui::Color32::LIGHT_RED, err);
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
                                ui.heading(format!("{} frames selected", selected_frame_ids.len()));
                                ui.separator();
                                ui.label("Multi-frame editing isn't supported yet.");
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

        // DMX Monitor (floating window, toggled via View menu).
        if self.show_dmx_monitor {
            egui::Window::new("DMX Monitor")
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

        // Status bar.
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let mut shared = self.dmx_shared.lock().unwrap();

                    // Blackout button — red when active.
                    let blackout_text = if shared.blackout {
                        egui::RichText::new("BLACKOUT").color(egui::Color32::WHITE).strong()
                    } else {
                        egui::RichText::new("Blackout").color(egui::Color32::from_gray(160))
                    };
                    let blackout_btn = egui::Button::new(blackout_text)
                        .fill(if shared.blackout { egui::Color32::from_rgb(180, 40, 40) } else { egui::Color32::TRANSPARENT });
                    if ui.add(blackout_btn).clicked() {
                        shared.blackout = !shared.blackout;
                    }

                    ui.separator();

                    // Bypass button — orange when active.
                    let bypass_text = if shared.bypass {
                        egui::RichText::new("BYPASS").color(egui::Color32::WHITE).strong()
                    } else {
                        egui::RichText::new("Bypass").color(egui::Color32::from_gray(160))
                    };
                    let bypass_btn = egui::Button::new(bypass_text)
                        .fill(if shared.bypass { egui::Color32::from_rgb(200, 140, 30) } else { egui::Color32::TRANSPARENT });
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
                    ui.colored_label(egui::Color32::from_gray(100), status);

                    let mut missing = self.input_controllers.missing_controllers();
                    missing.extend(self.audio_inputs.missing_inputs());
                    missing.extend(self.midi_hub.missing_devices());
                    if !missing.is_empty() {
                        ui.separator();
                        ui.colored_label(
                            crate::theme::SEM_WARNING,
                            format!("Not found: {}", missing.join(", ")),
                        );
                        if ui
                            .small_button("Rescan")
                            .on_hover_text("Look for the missing controllers and audio inputs again")
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
                        self.config.save();
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
                            egui::Color32::from_gray(140),
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
                            egui::Color32::from_rgb(200, 80, 80)
                        } else if tick_us > 500 {
                            egui::Color32::from_rgb(200, 160, 60)
                        } else {
                            egui::Color32::from_gray(110)
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
        // Routes to the setup undoer when the pointer is over the Setup
        // window, otherwise to the project undoer.
        let undo_pressed = !typing && ctx.input(|i| {
            i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z)
        });
        let redo_pressed = !typing && ctx.input(|i| {
            i.modifiers.command
                && ((i.modifiers.shift && i.key_pressed(egui::Key::Z))
                    || i.key_pressed(egui::Key::Y))
        });

        if undo_pressed {
            self.run_undo_action(if setup_hovered {
                UndoAction::UndoSetup
            } else {
                UndoAction::UndoProject
            });
        }
        if redo_pressed {
            self.run_undo_action(if setup_hovered {
                UndoAction::RedoSetup
            } else {
                UndoAction::RedoProject
            });
        }
        if let Some(action) = self.pending_undo_action.take() {
            self.run_undo_action(action);
        }

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
                        if ui.button("Save").clicked() { do_save = true; }
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
        self.config.save();
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
