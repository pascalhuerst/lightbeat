mod beat_clock;
mod color;
mod config;
mod dmx_io;
mod engine;
mod setup;
mod interfaces;
mod link_controller;
mod objects;
mod project;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};
use config::AppConfig;
use engine::types::{new_shared_state, EngineCommand, NodeId, PortType, ProcessNode, SubgraphInnerCmd};
use engine::EngineHandle;
use engine::nodes::display::color_display::ColorDisplayProcessNode;
use engine::nodes::display::scope::ScopeProcessNode;
use engine::nodes::display::value_display::ValueDisplayProcessNode;
use engine::nodes::io::clock::ClockProcessNode;
use engine::nodes::io::internal_clock::InternalClockProcessNode;
use engine::nodes::ui::button::ButtonProcessNode;
use engine::nodes::math::change_detect::ChangeDetectProcessNode;
use engine::nodes::math::color_ops::{ColorMergeProcessNode, ColorSplitProcessNode};
use engine::nodes::math::compare::{CompareOp, CompareProcessNode};
use engine::nodes::math::constant::ConstantProcessNode;
use engine::nodes::math::counter::CounterProcessNode;
use engine::nodes::math::lookup::LookupProcessNode;
use engine::nodes::math::logic_gate::{LogicOp, LogicGateProcessNode};
use engine::nodes::math::math_op::{MathOp, MathProcessNode};
use engine::nodes::math::palette_select::PaletteSelectProcessNode;
use engine::nodes::meta::subgraph::SubgraphProcessNode;
use engine::nodes::math::scaler::ScalerProcessNode;
use engine::nodes::math::oscillator::{OscFunc, OscillatorProcessNode};
use engine::nodes::math::position_ops::{PositionMergeProcessNode, PositionSplitProcessNode};
use engine::nodes::output::effect_stack::EffectStackProcessNode;
use engine::nodes::output::group::GroupProcessNode;
use engine::nodes::transport::clock_divider::ClockDividerProcessNode;
use engine::nodes::transport::clock_gen::ClockGenProcessNode;
use engine::nodes::transport::delay::TriggerDelayProcessNode;
use engine::nodes::transport::envelope::EnvelopeProcessNode;
use engine::nodes::transport::transition::TransitionProcessNode;
use engine::nodes::transport::lfo::LfoProcessNode;
use engine::nodes::transport::phase_scaler::PhaseScalerProcessNode;
use engine::nodes::transport::step_sequencer::StepSequencerProcessNode;
use widgets::nodes::display::color_display::ColorDisplayWidget;
use widgets::nodes::display::scope::ScopeWidget;
use widgets::nodes::display::value_display::ValueDisplayWidget;
use widgets::nodes::io::clock::ClockWidget;
use widgets::nodes::io::internal_clock::InternalClockWidget;
use widgets::nodes::ui::button::ButtonWidget;
use widgets::nodes::math::change_detect::ChangeDetectWidget;
use widgets::nodes::math::color_ops::{ColorMergeWidget, ColorSplitWidget};
use widgets::nodes::math::compare::CompareWidget;
use widgets::nodes::math::constant::ConstantWidget;
use widgets::nodes::math::counter::CounterWidget;
use widgets::nodes::math::lookup::LookupWidget;
use widgets::nodes::math::logic_gate::LogicGateWidget;
use widgets::nodes::math::math_op::MathWidget;
use widgets::nodes::math::palette_select::{PaletteSelectWidget, new_shared_palette_context};
use widgets::nodes::meta::subgraph::SubgraphWidget;
use widgets::nodes::math::scaler::ScalerWidget;
use widgets::nodes::math::oscillator::OscillatorWidget;
use widgets::nodes::math::position_ops::{PositionMergeWidget, PositionSplitWidget};
use widgets::nodes::output::effect_stack::EffectStackWidget;
use widgets::nodes::output::group::GroupWidget;
use widgets::nodes::transport::clock_divider::ClockDividerWidget;
use widgets::nodes::transport::clock_gen::ClockGenWidget;
use widgets::nodes::transport::delay::TriggerDelayWidget;
use widgets::nodes::transport::envelope::EnvelopeWidget;
use widgets::nodes::transport::transition::TransitionWidget;
use widgets::nodes::transport::lfo::LfoWidget;
use widgets::nodes::transport::phase_scaler::PhaseScalerWidget;
use widgets::nodes::transport::step_sequencer::StepSequencerWidget;
use widgets::nodes::NodeGraph;

/// Result from a background file dialog.
enum FileDialogResult {
    OpenProject(PathBuf),
    SaveProjectAs(PathBuf),
}

struct LightBeatApp {
    graph: NodeGraph,
    engine: EngineHandle,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
    config: AppConfig,
    project_path: Option<PathBuf>,
    file_dialog_rx: Option<std::sync::mpsc::Receiver<FileDialogResult>>,
    snapshot: Arc<std::sync::Mutex<beat_clock::LinkSnapshot>>,
    quit_requested: bool,
    show_dmx_monitor: bool,
    show_fixture_list: bool,
    show_object_list: bool,
    show_interface_list: bool,
    show_group_list: bool,
    show_color_palettes: bool,
    show_color_palette_groups: bool,
    dmx_monitor: widgets::dmx_monitor::DmxMonitor,
    dmx_shared: dmx_io::SharedDmxState,
    object_store: dmx_io::SharedObjectStore,
    group_ctx: widgets::nodes::output::group::SharedGroupContext,
    fixture_manager: widgets::fixture_list::FixtureManager,
    object_manager: widgets::object_list::ObjectManager,
    interface_manager: widgets::interface_list::InterfaceManager,
    group_manager: widgets::group_list::GroupManager,
    color_palette_manager: widgets::color_palette_list::ColorPaletteManager,
    color_palette_group_manager: widgets::color_palette_group_list::ColorPaletteGroupManager,
    palette_ctx: widgets::nodes::math::palette_select::SharedPaletteContext,
    project_undoer: egui::util::undoer::Undoer<project::ProjectFile>,
    setup_undoer: egui::util::undoer::Undoer<setup::SetupFile>,
}

impl LightBeatApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Add Phosphor icons as a fallback font so we can use icon glyphs
        // (constants like egui_phosphor::regular::ARROW_UP) anywhere in the UI.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let config = AppConfig::load();
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();
        let dmx_shared = dmx_io::new_shared_dmx_state();
        let object_store = dmx_io::new_shared_object_store();
        let group_ctx = widgets::nodes::output::group::new_shared_group_context();
        let engine = EngineHandle::start(dmx_shared.clone(), object_store.clone());

        let mut app = Self {
            graph: NodeGraph::new(),
            engine,
            beat_clock,
            _subs: Vec::new(),
            config,
            project_path: None,
            file_dialog_rx: None,
            snapshot,
            quit_requested: false,
            show_dmx_monitor: false,
            show_fixture_list: false,
            show_object_list: false,
            show_interface_list: false,
            show_group_list: false,
            show_color_palettes: false,
            show_color_palette_groups: false,
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
            project_undoer: egui::util::undoer::Undoer::default(),
            setup_undoer: egui::util::undoer::Undoer::default(),
        };

        // Load hardware setup (fixtures + interfaces).
        app.load_setup();

        app.register_node_factories();
        app.sync_group_context();
        app.sync_object_store();
        app.sync_interfaces();
        app.sync_palette_context();

        // Try autoload, or create default clock.
        let loaded = if app.config.autoload_on_open {
            let path = project::default_project_path();
            if path.exists() {
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
        }

        app
    }

    fn register_node_factories(&mut self) {
        // IO
        self.graph.register_node("IO", "Clock", |id| {
            Box::new(ClockWidget::new(id, new_shared_state(0, 3)))
        });
        self.graph.register_node("IO", "Internal Clock", |id| {
            Box::new(InternalClockWidget::new(id, new_shared_state(1, 3)))
        });

        // UI
        self.graph.register_node("UI", "Button", |id| {
            Box::new(ButtonWidget::new(id, new_shared_state(0, 1)))
        });

        // Transport
        self.graph.register_node("Transport", "Phase Scaler", |id| {
            Box::new(PhaseScalerWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "LFO", |id| {
            Box::new(LfoWidget::new(id, new_shared_state(1, 2)))
        });
        self.graph.register_node("Transport", "Step Sequencer", |id| {
            Box::new(StepSequencerWidget::new(id, new_shared_state(1, 3)))
        });
        self.graph.register_node("Transport", "ADSR", |id| {
            Box::new(EnvelopeWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Transport", "Trigger Delay", |id| {
            Box::new(TriggerDelayWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Clock Divider", |id| {
            Box::new(ClockDividerWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Clock Gen", |id| {
            Box::new(ClockGenWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Transition", |id| {
            // trigger(1) + phase(1) + palette(12) = 14 input channels, 12 output channels max
            Box::new(TransitionWidget::new(id, new_shared_state(14, 12)))
        });

        // Math
        self.graph.register_node("Math", "Add", |id| {
            Box::new(MathWidget::new(id, MathOp::Add, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Sub", |id| {
            Box::new(MathWidget::new(id, MathOp::Sub, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Mul", |id| {
            Box::new(MathWidget::new(id, MathOp::Mul, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Div", |id| {
            Box::new(MathWidget::new(id, MathOp::Div, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Sin", |id| {
            Box::new(OscillatorWidget::new(id, OscFunc::Sin, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Cos", |id| {
            Box::new(OscillatorWidget::new(id, OscFunc::Cos, new_shared_state(2, 1)))
        });
        self.graph.register_node("Color", "Color Merge", |id| {
            Box::new(ColorMergeWidget::new(id, new_shared_state(12, 12))) // Palette mode: 4×Color in, Palette out
        });
        self.graph.register_node("Color", "Color Split", |id| {
            Box::new(ColorSplitWidget::new(id, new_shared_state(12, 12))) // Palette mode: Palette in, 4×Color out
        });
        self.graph.register_node("Math", "Lookup", |id| {
            Box::new(LookupWidget::new(id, new_shared_state(1, 3))) // 1 input, up to 3 output channels (Color)
        });
        self.graph.register_node("Math", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, PortType::Untyped, new_shared_state(0, 1)))
        });
        self.graph.register_node("Math", "Const Logic", |id| {
            Box::new(ConstantWidget::new(id, PortType::Logic, new_shared_state(0, 1)))
        });
        self.graph.register_node("Math", "Const Phase", |id| {
            Box::new(ConstantWidget::new(id, PortType::Phase, new_shared_state(0, 1)))
        });

        self.graph.register_node("Math", "Counter", |id| {
            Box::new(CounterWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Math", "Change Detect", |id| {
            Box::new(ChangeDetectWidget::new(id, new_shared_state(2, 2)))
        });

        // Compare
        self.graph.register_node("Compare", ">=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Gte, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "<=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Lte, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "==", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Eq, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "!=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Neq, new_shared_state(2, 1)))
        });

        // Logic
        self.graph.register_node("Logic", "AND", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::And, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "OR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Or, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "XOR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Xor, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "NOT", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Not, new_shared_state(1, 1)))
        });

        // Position
        self.graph.register_node("Math", "Position Merge", |id| {
            Box::new(PositionMergeWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Math", "Position Split", |id| {
            Box::new(PositionSplitWidget::new(id, new_shared_state(2, 2)))
        });

        // Display
        self.graph.register_node("Display", "Scope", |id| {
            Box::new(ScopeWidget::new(id, new_shared_state(2, 0)))
        });
        self.graph.register_node("Color", "Color Display", |id| {
            Box::new(ColorDisplayWidget::new(id, new_shared_state(12, 0)))
        });
        self.graph.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, new_shared_state(1, 0)))
        });

        // Math - Scaler
        self.graph.register_node("Math", "Scaler", |id| {
            Box::new(ScalerWidget::new(id, new_shared_state(1, 1)))
        });

        // Color (palette)
        let pctx = self.palette_ctx.clone();
        self.graph.register_node("Color", "Palette Select", move |id| {
            // 2 inputs, Palette output = 12 channels
            Box::new(PaletteSelectWidget::new(id, new_shared_state(2, 12), pctx.clone()))
        });

        // Meta
        self.graph.register_node("Meta", "Subgraph", |id| {
            // Start with generous shared state; will resize as ports are added.
            Box::new(SubgraphWidget::new(id, new_shared_state(12, 12)))
        });

        // Output
        let gctx = self.group_ctx.clone();
        self.graph.register_node("Output", "Group Output", move |id| {
            Box::new(GroupWidget::new(id, new_shared_state(13, 0), gctx.clone()))
        });
        let gctx2 = self.group_ctx.clone();
        self.graph.register_node("Output", "Effect Stack", move |id| {
            // Generous initial channel buffer; engine resizes as layers change.
            Box::new(EffectStackWidget::new(id, new_shared_state(64, 0), gctx2.clone()))
        });
    }

    /// Sync group and object data to the shared context for Group Output widgets.
    fn sync_group_context(&self) {
        let mut ctx = self.group_ctx.lock().unwrap();
        ctx.groups = self.group_manager.groups.clone();
        ctx.objects = self.object_manager.objects.clone();
    }

    fn sync_palette_context(&self) {
        let mut ctx = self.palette_ctx.lock().unwrap();
        ctx.palettes = self.color_palette_manager.palettes.clone();
        ctx.groups = self.color_palette_group_manager.groups.clone();
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

    fn wire_new_nodes(&mut self) {
        // Handle nodes created by context menu / paste / duplicate.
        for new_node in self.graph.drain_new_nodes() {
            let node = self.graph.node_mut_at_path(new_node.index, &new_node.subgraph_path);
            let type_name = node.type_name();
            let id = node.node_id();
            let shared = Arc::clone(node.shared_state());
            let subgraph_path = new_node.subgraph_path;

            // Create corresponding engine node.
            let engine_node: Option<Box<dyn ProcessNode>> = match type_name {
                "Clock" => {
                    let engine_node = ClockProcessNode::new(id, Arc::clone(&self.snapshot));
                    let beat_state = engine_node.beat_state.clone();
                    self._subs.push(
                        self.beat_clock
                            .subscribe(BeatPattern::every(1), beat_state),
                    );
                    Some(Box::new(engine_node))
                }
                "Internal Clock" => Some(Box::new(InternalClockProcessNode::new(id))),
                "Button" => Some(Box::new(ButtonProcessNode::new(id))),
                "Phase Scaler" => Some(Box::new(PhaseScalerProcessNode::new(id))),
                "LFO" => Some(Box::new(LfoProcessNode::new(id))),
                "Step Sequencer" => Some(Box::new(StepSequencerProcessNode::new(id))),
                "Scope" => Some(Box::new(ScopeProcessNode::new(id))),
                "ADSR" => Some(Box::new(EnvelopeProcessNode::new(id))),
                "Trigger Delay" => Some(Box::new(TriggerDelayProcessNode::new(id))),
                "Clock Divider" => Some(Box::new(ClockDividerProcessNode::new(id))),
                "Clock Gen" => Some(Box::new(ClockGenProcessNode::new(id))),
                "Transition" => Some(Box::new(TransitionProcessNode::new(id))),
                "Add" | "Sub" | "Mul" | "Div" => {
                    let op = match type_name {
                        "Add" => MathOp::Add, "Sub" => MathOp::Sub,
                        "Mul" => MathOp::Mul, "Div" => MathOp::Div,
                        _ => unreachable!(),
                    };
                    Some(Box::new(MathProcessNode::new(id, op)))
                }
                "Color Display" => Some(Box::new(ColorDisplayProcessNode::new(id))),
                "Value Display" => Some(Box::new(ValueDisplayProcessNode::new(id))),
                "Palette Select" => Some(Box::new(PaletteSelectProcessNode::new(id))),
                "Scaler" => Some(Box::new(ScalerProcessNode::new(id))),
                ">=" | "<=" | "==" | "!=" => {
                    let op = match type_name {
                        ">=" => CompareOp::Gte, "<=" => CompareOp::Lte,
                        "==" => CompareOp::Eq, "!=" => CompareOp::Neq,
                        _ => unreachable!(),
                    };
                    Some(Box::new(CompareProcessNode::new(id, op)))
                }
                "AND" | "OR" | "XOR" | "NOT" => {
                    let op = match type_name {
                        "AND" => LogicOp::And, "OR" => LogicOp::Or,
                        "XOR" => LogicOp::Xor, "NOT" => LogicOp::Not,
                        _ => unreachable!(),
                    };
                    Some(Box::new(LogicGateProcessNode::new(id, op)))
                }
                "Color Merge" => Some(Box::new(ColorMergeProcessNode::new(id))),
                "Color Split" => Some(Box::new(ColorSplitProcessNode::new(id))),
                "Position Merge" => Some(Box::new(PositionMergeProcessNode::new(id))),
                "Position Split" => Some(Box::new(PositionSplitProcessNode::new(id))),
                "Lookup" => Some(Box::new(LookupProcessNode::new(id))),
                "Counter" => Some(Box::new(CounterProcessNode::new(id))),
                "Change Detect" => Some(Box::new(ChangeDetectProcessNode::new(id))),
                "Const Value" => Some(Box::new(ConstantProcessNode::new(id, PortType::Untyped, 0.0))),
                "Const Logic" => Some(Box::new(ConstantProcessNode::new(id, PortType::Logic, 0.0))),
                "Const Phase" => Some(Box::new(ConstantProcessNode::new(id, PortType::Phase, 0.0))),
                "Sin" => Some(Box::new(OscillatorProcessNode::new(id, OscFunc::Sin))),
                "Cos" => Some(Box::new(OscillatorProcessNode::new(id, OscFunc::Cos))),
                "Subgraph" => Some(Box::new(SubgraphProcessNode::new(id))),
                "Group Output" => Some(Box::new(GroupProcessNode::new(id, self.object_store.clone()))),
                "Effect Stack" => Some(Box::new(EffectStackProcessNode::new(id, self.object_store.clone()))),
                _ => {
                    eprintln!("Unknown node type for engine: {}", type_name);
                    None
                }
            };

            if let Some(engine_node) = engine_node {
                if subgraph_path.is_empty() {
                    // Root level — send directly.
                    self.engine.send(EngineCommand::AddNode {
                        node: engine_node,
                        shared,
                    });
                } else {
                    // Inside a subgraph — wrap in SubgraphInnerCommand.
                    self.engine.send(EngineCommand::SubgraphInnerCommand {
                        subgraph_path,
                        command: Box::new(SubgraphInnerCmd::AddNode {
                            node: engine_node,
                            shared,
                        }),
                    });
                }
            }
        }
    }

    fn load_setup(&mut self) {
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
                // Reset history so undo doesn't go back to an empty setup.
                self.setup_undoer = egui::util::undoer::Undoer::default();
            }
            Err(e) => eprintln!("Failed to load setup: {}", e),
        }
    }

    fn save_setup(&self) {
        let setup = self.current_setup();
        if let Err(e) = setup::save_setup(&setup) {
            eprintln!("Failed to save setup: {}", e);
        }
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

    fn save_project(&mut self) {
        let path = self
            .project_path
            .clone()
            .unwrap_or_else(project::default_project_path);
        if let Err(e) = project::save_to_file(&self.graph, &path) {
            eprintln!("Failed to save project: {}", e);
        } else {
            self.project_path = Some(path);
        }
    }

    fn save_project_as(&mut self, ctx: &egui::Context) {
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

    fn poll_file_dialog(&mut self) {
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
                    FileDialogResult::SaveProjectAs(path) => {
                        if let Err(e) = project::save_to_file(&self.graph, &path) {
                            eprintln!("Failed to save project: {}", e);
                        } else {
                            self.project_path = Some(path);
                        }
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Dialog was cancelled (thread finished without sending a result).
                self.file_dialog_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Dialog still open, keep waiting.
            }
        }
    }

    fn send_load_data_recursive(
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
            if saved.type_name == "Subgraph" {
                if let Some(inner_project) = &saved.inner_graph {
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
    }

    /// Apply an in-memory ProjectFile, replacing the current graph & engine state.
    /// Used both for file-load and for undo/redo. Preserves pan/zoom and the
    /// active subgraph path across the rebuild.
    fn apply_project(&mut self, proj: &project::ProjectFile) {
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

    fn load_project_from(&mut self, path: &std::path::Path) {
        match project::load_from_file(&path.to_path_buf()) {
            Ok(proj) => {
                self.apply_project(&proj);
                self.graph.fit_to_content();
                self.project_path = Some(path.to_path_buf());
                // Reset history so undo doesn't go back to the empty pre-load graph.
                self.project_undoer = egui::util::undoer::Undoer::default();
            }
            Err(e) => {
                eprintln!("Failed to open project: {}", e);
            }
        }
    }

    /// Build current SetupFile snapshot (used for both saving and feeding the undoer).
    fn current_setup(&self) -> setup::SetupFile {
        setup::SetupFile {
            fixtures: self.fixture_manager.fixtures.clone(),
            objects: self.object_manager.objects.clone(),
            interfaces: self.interface_manager.to_saved(),
            groups: self.group_manager.groups.clone(),
            color_palettes: self.color_palette_manager.palettes.clone(),
            color_palette_groups: self.color_palette_group_manager.groups.clone(),
        }
    }

    /// Apply an in-memory SetupFile, rebuilding all managers and re-syncing engine state.
    fn apply_setup(&mut self, s: &setup::SetupFile) {
        self.fixture_manager = widgets::fixture_list::FixtureManager::from_fixtures(s.fixtures.clone());
        self.object_manager = widgets::object_list::ObjectManager::from_objects(s.objects.clone());
        self.interface_manager = widgets::interface_list::InterfaceManager::from_saved(s.interfaces.clone());
        self.group_manager = widgets::group_list::GroupManager::from_groups(s.groups.clone());
        self.color_palette_manager = widgets::color_palette_list::ColorPaletteManager::from_palettes(s.color_palettes.clone());
        self.color_palette_group_manager = widgets::color_palette_group_list::ColorPaletteGroupManager::from_groups(s.color_palette_groups.clone());

        self.sync_group_context();
        self.sync_object_store();
        self.sync_interfaces();
        self.sync_palette_context();
    }

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

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // Poll background file dialog results.
        self.poll_file_dialog();

        // Snapshot current state for the undoers and the Edit menu.
        // Built once per frame; cheap (clones small structs).
        let now = ctx.input(|i| i.time);
        let current_project = project::save_graph(&self.graph);
        let current_setup = self.current_setup();
        self.project_undoer.feed_state(now, &current_project);
        self.setup_undoer.feed_state(now, &current_setup);

        let can_undo_proj = self.project_undoer.has_undo(&current_project);
        let can_redo_proj = self.project_undoer.has_redo(&current_project);
        let can_undo_setup = self.setup_undoer.has_undo(&current_setup);
        let can_redo_setup = self.setup_undoer.has_redo(&current_setup);

        // Menu bar.
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        ui.close_menu();
                        self.open_project(ctx);
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        ui.close_menu();
                        self.save_project();
                    }
                    if ui.button("Save As...").clicked() {
                        ui.close_menu();
                        self.save_project_as(ctx);
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
                        if let Some(p) = self.project_undoer.undo(&current_project).cloned() {
                            self.apply_project(&p);
                        }
                    }
                    if ui.add_enabled(can_redo_proj, egui::Button::new("Redo Project")).clicked() {
                        ui.close_menu();
                        if let Some(p) = self.project_undoer.redo(&current_project).cloned() {
                            self.apply_project(&p);
                        }
                    }
                    ui.separator();
                    if ui.add_enabled(can_undo_setup, egui::Button::new("Undo Setup")).clicked() {
                        ui.close_menu();
                        if let Some(s) = self.setup_undoer.undo(&current_setup).cloned() {
                            self.apply_setup(&s);
                        }
                    }
                    if ui.add_enabled(can_redo_setup, egui::Button::new("Redo Setup")).clicked() {
                        ui.close_menu();
                        if let Some(s) = self.setup_undoer.redo(&current_setup).cloned() {
                            self.apply_setup(&s);
                        }
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_fixture_list, "Fixture Templates").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_object_list, "Objects").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_group_list, "Groups").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_interface_list, "Interfaces").changed() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.show_color_palettes, "Color Palettes").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_color_palette_groups, "Color Palette Groups").changed() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.show_dmx_monitor, "DMX Monitor").changed() {
                        ui.close_menu();
                    }
                });
            });
        });

        // Track whether the pointer is over any setup window. This routes
        // Ctrl+Z to the setup undoer instead of the project undoer.
        let mut setup_hovered = false;
        let mut mark_hovered = |r: Option<egui::InnerResponse<Option<()>>>| {
            if let Some(ir) = r {
                if ir.response.contains_pointer() { setup_hovered = true; }
            }
        };

        // Fixture templates window.
        if self.show_fixture_list {
            let r = egui::Window::new("Fixture Templates")
                .open(&mut self.show_fixture_list)
                .default_size([350.0, 400.0])
                .show(ctx, |ui| {
                    self.fixture_manager.show(ui);
                });
            mark_hovered(r);
        }

        // Objects window.
        if self.show_object_list {
            let interface_names: Vec<(u32, String)> = self.interface_manager.interfaces
                .iter()
                .map(|e| (e.id, e.name.clone()))
                .collect();
            let r = egui::Window::new("Objects")
                .open(&mut self.show_object_list)
                .default_size([400.0, 400.0])
                .show(ctx, |ui| {
                    self.object_manager.show(ui, &self.fixture_manager.fixtures, &interface_names);
                });
            mark_hovered(r);
        }

        // Interface list window (toggled via View menu).
        if self.show_interface_list {
            let r = egui::Window::new("Interfaces")
                .open(&mut self.show_interface_list)
                .default_size([350.0, 300.0])
                .show(ctx, |ui| {
                    self.interface_manager.show(ui);
                });
            mark_hovered(r);
        }

        // Groups window (toggled via View menu).
        if self.show_group_list {
            let r = egui::Window::new("Groups")
                .open(&mut self.show_group_list)
                .default_size([350.0, 400.0])
                .show(ctx, |ui| {
                    self.group_manager.show(ui, &self.object_manager.objects);
                });
            mark_hovered(r);
        }

        // Color Palettes window.
        if self.show_color_palettes {
            let r = egui::Window::new("Color Palettes")
                .open(&mut self.show_color_palettes)
                .default_size([300.0, 400.0])
                .show(ctx, |ui| {
                    self.color_palette_manager.show(ui);
                });
            mark_hovered(r);
        }

        // Color Palette Groups window.
        if self.show_color_palette_groups {
            let r = egui::Window::new("Color Palette Groups")
                .open(&mut self.show_color_palette_groups)
                .default_size([350.0, 400.0])
                .show(ctx, |ui| {
                    self.color_palette_group_manager.show(ui, &self.color_palette_manager.palettes);
                });
            mark_hovered(r);
        }

        // Inspector panel.
        egui::SidePanel::right("inspector")
            .default_width(250.0)
            .show(ctx, |ui| {
                let selected = self.graph.selected_nodes_mut();
                if selected.is_empty() {
                    ui.heading("Inspector");
                    ui.separator();
                    ui.label("Select a node to inspect.");
                } else if selected.len() == 1 {
                    let node = &mut *selected.into_iter().next().unwrap();
                    widgets::inspector::show_inspector(ui, node.as_mut());
                } else {
                    ui.heading(format!("{} nodes selected", selected.len()));
                    ui.separator();
                    widgets::inspector::show_multi_inspector(ui, selected);
                }
            });

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

                    // Right-aligned zoom indicator.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.colored_label(
                            egui::Color32::from_gray(140),
                            format!("Zoom: {:.0}%", self.graph.zoom() * 100.0),
                        );
                    });
                });
            });

        // Node graph.
        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui, self.config.snap_to_grid);
        });

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

        // Send any pending engine commands from the graph.
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }

        // Keyboard shortcuts.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_project();
            self.save_setup();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.open_project(ctx);
        }

        // Undo / Redo: Ctrl+Z / Ctrl+Shift+Z (and Ctrl+Y for redo).
        // Routes to the setup undoer if the pointer is over a setup window,
        // otherwise to the project undoer.
        let undo_pressed = ctx.input(|i| {
            i.modifiers.ctrl && !i.modifiers.shift && i.key_pressed(egui::Key::Z)
        });
        let redo_pressed = ctx.input(|i| {
            i.modifiers.ctrl
                && ((i.modifiers.shift && i.key_pressed(egui::Key::Z))
                    || i.key_pressed(egui::Key::Y))
        });

        if undo_pressed {
            if setup_hovered {
                if let Some(s) = self.setup_undoer.undo(&current_setup).cloned() {
                    self.apply_setup(&s);
                }
            } else if let Some(p) = self.project_undoer.undo(&current_project).cloned() {
                self.apply_project(&p);
            }
        }
        if redo_pressed {
            if setup_hovered {
                if let Some(s) = self.setup_undoer.redo(&current_setup).cloned() {
                    self.apply_setup(&s);
                }
            } else if let Some(p) = self.project_undoer.redo(&current_project).cloned() {
                self.apply_project(&p);
            }
        }

        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.config.autosave_on_close {
            self.save_project();
            self.save_setup();
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
