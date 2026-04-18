mod audio;
mod beat_clock;
mod color;
mod config;
mod dmx_io;
mod engine;
mod input_controller;
mod macros;
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
use audio::manager::AudioInputManager;
use config::{AppConfig, InspectorMode};
use macros::library::LibraryManager;
use macros::Macro;
use engine::types::{new_shared_state, EngineCommand, NodeId, PortType, ProcessNode, SubgraphInnerCmd};
use engine::EngineHandle;
use input_controller::InputControllerManager;
use engine::nodes::display::color_display::ColorDisplayProcessNode;
use engine::nodes::display::scope::ScopeProcessNode;
use engine::nodes::display::led_display::LedDisplayProcessNode;
use engine::nodes::display::value_display::ValueDisplayProcessNode;
use engine::nodes::io::audio_input::AudioInputProcessNode;
use engine::nodes::io::clock::ClockProcessNode;
use engine::nodes::io::input_controller::InputControllerProcessNode;
use engine::nodes::io::push1::Push1ProcessNode;
use engine::nodes::io::internal_clock::InternalClockProcessNode;
use engine::nodes::ui::button::ButtonProcessNode;
use engine::nodes::ui::button_group::ButtonGroupProcessNode;
use engine::nodes::ui::fader::FaderProcessNode;
use engine::nodes::ui::fader_group::FaderGroupProcessNode;
use engine::nodes::ui::peak_meter::PeakMeterProcessNode;
use engine::nodes::math::change_detect::ChangeDetectProcessNode;
use engine::nodes::math::flipflop::{FlipFlopProcessNode, JkFlipFlopProcessNode};
use engine::nodes::math::color_modifier::ColorModifierProcessNode;
use engine::nodes::math::color_ops::{ColorMergeProcessNode, ColorSplitProcessNode};
use engine::nodes::math::compare::{CompareOp, CompareProcessNode};
use engine::nodes::math::constant::ConstantProcessNode;
use engine::nodes::math::counter::CounterProcessNode;
use engine::nodes::math::gradient_source::GradientSourceProcessNode;
use engine::nodes::math::lookup::LookupProcessNode;
use engine::nodes::math::logic_gate::{LogicOp, LogicGateProcessNode};
use engine::nodes::math::math_op::{MathOp, MathProcessNode};
use engine::nodes::math::multiplex::{DemultiplexerProcessNode, MultiplexerProcessNode};
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
use engine::nodes::transport::hold::TriggerHoldProcessNode;
use engine::nodes::transport::latch::LatchProcessNode;
use engine::nodes::transport::sample_hold::SampleHoldProcessNode;
use engine::nodes::transport::transition::TransitionProcessNode;
use engine::nodes::transport::lfo::LfoProcessNode;
use engine::nodes::transport::phase_scaler::PhaseScalerProcessNode;
use engine::nodes::transport::step_sequencer::StepSequencerProcessNode;
use widgets::nodes::display::color_display::ColorDisplayWidget;
use widgets::nodes::display::scope::ScopeWidget;
use widgets::nodes::display::led_display::LedDisplayWidget;
use widgets::nodes::display::value_display::ValueDisplayWidget;
use widgets::nodes::io::audio_input::AudioInputWidget;
use widgets::nodes::io::clock::ClockWidget;
use widgets::nodes::io::input_controller::InputControllerWidget;
use widgets::nodes::io::push1::Push1Widget;
use widgets::nodes::io::internal_clock::InternalClockWidget;
use widgets::nodes::ui::button::ButtonWidget;
use widgets::nodes::ui::button_group::ButtonGroupWidget;
use widgets::nodes::ui::fader::FaderWidget;
use widgets::nodes::ui::fader_group::FaderGroupWidget;
use widgets::nodes::ui::peak_meter::PeakMeterWidget;
use widgets::nodes::math::change_detect::ChangeDetectWidget;
use widgets::nodes::math::flipflop::{FlipFlopKind, FlipFlopWidget};
use widgets::nodes::math::color_modifier::ColorModifierWidget;
use widgets::nodes::math::color_ops::{ColorMergeWidget, ColorSplitWidget};
use widgets::nodes::math::compare::CompareWidget;
use widgets::nodes::math::constant::ConstantWidget;
use widgets::nodes::math::counter::CounterWidget;
use widgets::nodes::math::gradient_source::GradientSourceWidget;
use widgets::nodes::math::lookup::LookupWidget;
use widgets::nodes::math::logic_gate::LogicGateWidget;
use widgets::nodes::math::math_op::MathWidget;
use widgets::nodes::math::multiplex::{DemultiplexerWidget, MultiplexerWidget};
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
use widgets::nodes::transport::hold::TriggerHoldWidget;
use widgets::nodes::transport::latch::LatchWidget;
use widgets::nodes::transport::sample_hold::SampleHoldWidget;
use widgets::nodes::transport::transition::TransitionWidget;
use widgets::nodes::transport::lfo::LfoWidget;
use widgets::nodes::transport::phase_scaler::PhaseScalerWidget;
use widgets::nodes::transport::step_sequencer::StepSequencerWidget;
use widgets::nodes::graph::MacroRequest;
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
    input_controllers: InputControllerManager,
    show_input_controllers: bool,
    audio_inputs: AudioInputManager,
    /// Shared map of published Portal In names → values. Referenced by both
    /// Portal In and Portal Out process nodes so data flows between them
    /// without visible wires.
    portals: engine::nodes::meta::portal::SharedPortalRegistry,
    show_audio_inputs: bool,
    library: LibraryManager,
    show_library: bool,
    library_search: String,
    save_macro_dialog: Option<SaveMacroDialog>,
    project_undoer: egui::util::undoer::Undoer<project::ProjectFile>,
    setup_undoer: egui::util::undoer::Undoer<setup::SetupFile>,
}

/// State for the modal "Save as macro" dialog.
struct SaveMacroDialog {
    /// Subgraph node id whose inner graph we're saving.
    target_node: NodeId,
    /// Subgraph path to the target's *parent* level (i.e., the level the
    /// target node lives in). Empty = root.
    parent_path: Vec<NodeId>,
    name: String,
    group: String,
    description: String,
    /// Comma-separated tag input.
    tags: String,
    error: Option<String>,
    /// When set, "Save" overwrites this existing macro file. Name/group
    /// become read-only since the destination path is already fixed.
    overwrite_path: Option<std::path::PathBuf>,
}

/// Payload carried during a macro-library → canvas drag.
#[derive(Clone)]
struct MacroDragPayload {
    path: std::path::PathBuf,
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
            input_controllers: InputControllerManager::new(),
            show_input_controllers: false,
            audio_inputs: AudioInputManager::new(),
            show_audio_inputs: false,
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
            // 4 inputs (play/stop, bpm, set bpm, reset) and 3 outputs.
            Box::new(InternalClockWidget::new(id, new_shared_state(4, 3)))
        });
        let ic_shared = self.input_controllers.shared.clone();
        self.graph.register_node("IO", "Input Controller", move |id| {
            // Generous channel budget on both sides — the engine resizes its
            // active port list as the bound controller's layout changes. 192
            // covers generic MIDI and BCF2000 with headroom; Push 1 uses its
            // own dedicated node.
            Box::new(InputControllerWidget::new(id, new_shared_state(192, 192), ic_shared.clone()))
        });
        let ic_shared_push = self.input_controllers.shared.clone();
        self.graph.register_node("IO", "Push 1", move |id| {
            // 25 outputs, 21 inputs (see push1.rs). 32/32 gives headroom.
            Box::new(Push1Widget::new(id, new_shared_state(32, 32), ic_shared_push.clone()))
        });
        let ai_shared = self.audio_inputs.shared.clone();
        self.graph.register_node("IO", "Audio Input", move |id| {
            Box::new(AudioInputWidget::new(id, new_shared_state(0, 16), ai_shared.clone()))
        });

        // UI
        self.graph.register_node("UI", "Button", |id| {
            // 1 optional input (Logic), 1 output.
            Box::new(ButtonWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("UI", "Fader", |id| {
            // 1 optional input (when "Enable input" is checked), 1 output.
            Box::new(FaderWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("UI", "Button Group", |id| {
            Box::new(ButtonGroupWidget::new(id, new_shared_state(256, 256)))
        });
        self.graph.register_node("UI", "Fader Group", |id| {
            Box::new(FaderGroupWidget::new(id, new_shared_state(256, 256)))
        });
        self.graph.register_node("UI", "Peak Level Meter", |id| {
            Box::new(PeakMeterWidget::new(id, new_shared_state(2, 0)))
        });

        // Transport
        self.graph.register_node("Transport", "Phase Scaler", |id| {
            Box::new(PhaseScalerWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "LFO", |id| {
            Box::new(LfoWidget::new(id, new_shared_state(1, 2)))
        });
        self.graph.register_node("Transport", "Step Sequencer", |id| {
            Box::new(StepSequencerWidget::new(id, new_shared_state(2, 3)))
        });
        self.graph.register_node("Transport", "ADSR", |id| {
            Box::new(EnvelopeWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Transport", "Trigger Delay", |id| {
            Box::new(TriggerDelayWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Trigger Hold", |id| {
            Box::new(TriggerHoldWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Sample & Hold", |id| {
            Box::new(SampleHoldWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Latch", |id| {
            Box::new(LatchWidget::new(id, new_shared_state(1, 1)))
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

        // Math (variadic — generous initial input buffer; engine grows as ports are added).
        self.graph.register_node("Math", "Add", |id| {
            Box::new(MathWidget::new(id, MathOp::Add, new_shared_state(16, 1)))
        });
        self.graph.register_node("Math", "Sub", |id| {
            Box::new(MathWidget::new(id, MathOp::Sub, new_shared_state(16, 1)))
        });
        self.graph.register_node("Math", "Mul", |id| {
            Box::new(MathWidget::new(id, MathOp::Mul, new_shared_state(16, 1)))
        });
        self.graph.register_node("Math", "Div", |id| {
            Box::new(MathWidget::new(id, MathOp::Div, new_shared_state(16, 1)))
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

        // Logic (AND/OR/XOR variadic; NOT is unary).
        self.graph.register_node("Logic", "AND", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::And, new_shared_state(16, 1)))
        });
        self.graph.register_node("Logic", "OR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Or, new_shared_state(16, 1)))
        });
        self.graph.register_node("Logic", "XOR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Xor, new_shared_state(16, 1)))
        });
        self.graph.register_node("Logic", "NOT", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Not, new_shared_state(1, 1)))
        });
        self.graph.register_node("Logic", "Flip-Flop", |id| {
            Box::new(FlipFlopWidget::new(id, new_shared_state(2, 2), FlipFlopKind::Sr))
        });
        self.graph.register_node("Logic", "JK Flip-Flop", |id| {
            Box::new(FlipFlopWidget::new(id, new_shared_state(3, 2), FlipFlopKind::Jk))
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
            // Sized for the largest mode (Gradient = 40 channels).
            Box::new(ColorDisplayWidget::new(id, new_shared_state(40, 0)))
        });
        self.graph.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, new_shared_state(1, 0)))
        });
        self.graph.register_node("Display", "LED Display", |id| {
            Box::new(LedDisplayWidget::new(id, new_shared_state(1, 0)))
        });

        // Math - Scaler
        self.graph.register_node("Math", "Scaler", |id| {
            Box::new(ScalerWidget::new(id, new_shared_state(1, 1)))
        });

        // Math - Multiplexer / Demultiplexer (generic typed routing).
        // Channel budgets sized for the worst case: MUX_MAX_SLOTS × Gradient (40 ch).
        self.graph.register_node("Math", "Multiplexer", |id| {
            // 1 select + MUX_MAX_SLOTS * 40 inputs, 40 outputs.
            Box::new(MultiplexerWidget::new(
                id,
                new_shared_state(
                    1 + engine::nodes::math::multiplex::MUX_MAX_SLOTS * 40,
                    40,
                ),
            ))
        });
        self.graph.register_node("Math", "Demultiplexer", |id| {
            // 1 select + 40 in, MUX_MAX_SLOTS * 40 outputs.
            Box::new(DemultiplexerWidget::new(
                id,
                new_shared_state(
                    1 + 40,
                    engine::nodes::math::multiplex::MUX_MAX_SLOTS * 40,
                ),
            ))
        });

        // Color (palette)
        let pctx = self.palette_ctx.clone();
        self.graph.register_node("Color", "Palette Select", move |id| {
            // 2 inputs, Palette output = 12 channels
            Box::new(PaletteSelectWidget::new(id, new_shared_state(2, 12), pctx.clone()))
        });
        self.graph.register_node("Color", "Gradient Source", |id| {
            // No inputs, Gradient output = 40 channels (8 stops × 5 floats).
            Box::new(GradientSourceWidget::new(id, new_shared_state(0, 40)))
        });
        self.graph.register_node("Color", "Color Modifier", |id| {
            // Sized for the largest mode (Gradient = 40 ch main + 1 amount), same out.
            Box::new(ColorModifierWidget::new(id, new_shared_state(40 + 1, 40)))
        });

        // Meta
        self.graph.register_node("Meta", "Subgraph", |id| {
            // Start with generous shared state; will resize as ports are added.
            Box::new(SubgraphWidget::new(id, new_shared_state(12, 12)))
        });
        self.graph.register_node("Meta", "Portal In", |id| {
            // Generous channel budget — user can add many ports of any type.
            Box::new(widgets::nodes::meta::portal::PortalInWidget::new(
                id, new_shared_state(64, 0),
            ))
        });
        let portals_for_out = self.portals.clone();
        self.graph.register_node("Meta", "Portal Out", move |id| {
            Box::new(widgets::nodes::meta::portal::PortalOutWidget::new(
                id, new_shared_state(0, 64), portals_for_out.clone(),
            ))
        });

        // Output
        let gctx = self.group_ctx.clone();
        self.graph.register_node("Output", "Group Output", move |id| {
            // Sized for the largest mode: Triggered (trigger + select + width + gradient) = 43 channels.
            Box::new(GroupWidget::new(id, new_shared_state(43, 0), gctx.clone()))
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
                "Input Controller" => Some(Box::new(InputControllerProcessNode::new(
                    id, self.input_controllers.shared.clone(),
                ))),
                "Push 1" => Some(Box::new(Push1ProcessNode::new(
                    id, self.input_controllers.shared.clone(),
                ))),
                "Audio Input" => Some(Box::new(AudioInputProcessNode::new(
                    id, self.audio_inputs.shared.clone(),
                ))),
                "Button" => Some(Box::new(ButtonProcessNode::new(id))),
                "Fader" => Some(Box::new(FaderProcessNode::new(id))),
                "Button Group" => Some(Box::new(ButtonGroupProcessNode::new(id))),
                "Fader Group" => Some(Box::new(FaderGroupProcessNode::new(id))),
                "Peak Level Meter" => Some(Box::new(PeakMeterProcessNode::new(id))),
                "Phase Scaler" => Some(Box::new(PhaseScalerProcessNode::new(id))),
                "LFO" => Some(Box::new(LfoProcessNode::new(id))),
                "Step Sequencer" => Some(Box::new(StepSequencerProcessNode::new(id))),
                "Scope" => Some(Box::new(ScopeProcessNode::new(id))),
                "ADSR" => Some(Box::new(EnvelopeProcessNode::new(id))),
                "Trigger Delay" => Some(Box::new(TriggerDelayProcessNode::new(id))),
                "Trigger Hold" => Some(Box::new(TriggerHoldProcessNode::new(id))),
                "Sample & Hold" => Some(Box::new(SampleHoldProcessNode::new(id))),
                "Latch" => Some(Box::new(LatchProcessNode::new(id))),
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
                "LED Display" => Some(Box::new(LedDisplayProcessNode::new(id))),
                "Palette Select" => Some(Box::new(PaletteSelectProcessNode::new(id))),
                "Gradient Source" => Some(Box::new(GradientSourceProcessNode::new(id))),
                "Color Modifier" => Some(Box::new(ColorModifierProcessNode::new(id))),
                "Multiplexer" => Some(Box::new(MultiplexerProcessNode::new(id))),
                "Demultiplexer" => Some(Box::new(DemultiplexerProcessNode::new(id))),
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
                "Flip-Flop" => Some(Box::new(FlipFlopProcessNode::new(id))),
                "JK Flip-Flop" => Some(Box::new(JkFlipFlopProcessNode::new(id))),
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
                "Portal In" => Some(Box::new(
                    engine::nodes::meta::portal::PortalInProcessNode::new(id, self.portals.clone()),
                )),
                "Portal Out" => Some(Box::new(
                    engine::nodes::meta::portal::PortalOutProcessNode::new(id, self.portals.clone()),
                )),
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
                self.input_controllers.set_controllers(&s.input_controllers);
                self.audio_inputs.set_inputs(&s.audio_inputs);
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

    /// Clear the graph to an empty project and forget the current file path.
    /// The Setup (fixtures, interfaces, palettes, etc.) is left alone —
    /// that's a separate file and the user may want to keep working with
    /// the same hardware set-up across projects.
    fn new_project(&mut self) {
        let empty = project::ProjectFile {
            nodes: Vec::new(),
            connections: Vec::new(),
        };
        self.apply_project(&empty);
        self.project_path = None;
        // Drop undo history so the first undo doesn't resurrect the previous
        // project's graph.
        self.project_undoer = egui::util::undoer::Undoer::default();
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
            input_controllers: self.input_controllers.export(),
            audio_inputs: self.audio_inputs.export(),
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
        self.input_controllers.set_controllers(&s.input_controllers);
        self.audio_inputs.set_inputs(&s.audio_inputs);

        self.sync_group_context();
        self.sync_object_store();
        self.sync_interfaces();
        self.sync_palette_context();
    }

    // -- Macro library helpers ----------------------------------------------

    fn show_library_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Macros");
            if ui.small_button(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .on_hover_text("Refresh library")
                .clicked()
            {
                self.library.rescan();
            }
        });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
            ui.add(egui::TextEdit::singleline(&mut self.library_search)
                .hint_text("search name/tag/group")
                .desired_width(ui.available_width()));
        });
        ui.separator();

        if let Some(err) = &self.library.last_error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }
        if self.library.entries.is_empty() {
            ui.colored_label(egui::Color32::from_gray(140),
                format!("No macros yet.\nSave one via right-click on a Subgraph.\n\nLibrary at: {}",
                    self.library.root.display()));
            return;
        }

        let q = self.library_search.to_lowercase();
        let entries: Vec<_> = self.library.entries.iter().enumerate()
            .filter(|(_, e)| {
                if q.is_empty() { return true; }
                e.name.to_lowercase().contains(&q)
                    || e.group.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .map(|(i, e)| (i, e.clone()))
            .collect();

        let mut delete: Option<std::path::PathBuf> = None;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // Group entries by their group path.
            let mut current_group: Option<String> = None;
            for (_idx, e) in &entries {
                if Some(&e.group) != current_group.as_ref() {
                    if current_group.is_some() { ui.add_space(4.0); }
                    let label = if e.group.is_empty() { "(root)".to_string() } else { e.group.clone() };
                    ui.colored_label(egui::Color32::from_gray(150),
                        egui::RichText::new(label).strong().size(11.0));
                    current_group = Some(e.group.clone());
                }
                let dnd_id = egui::Id::new(("macro-dnd", &e.path));
                let payload = MacroDragPayload { path: e.path.clone() };
                let inner_resp = ui.dnd_drag_source(dnd_id, payload, |ui| {
                    ui.add(egui::Button::new(&e.name).wrap_mode(egui::TextWrapMode::Extend))
                });
                let mut hover = format!("Drag onto canvas to add.\n\n{}", e.description);
                if !e.tags.is_empty() {
                    hover.push_str(&format!("\n\nTags: {}", e.tags.join(", ")));
                }
                let btn_resp = inner_resp.inner.on_hover_text(hover);
                btn_resp.context_menu(|ui| {
                    if ui.button("Delete from library").clicked() {
                        delete = Some(e.path.clone());
                        ui.close_menu();
                    }
                });
            }
        });

        if let Some(p) = delete {
            if let Err(e) = self.library.delete(&p) {
                eprintln!("delete macro: {}", e);
            }
        }
    }

    /// Handle a macro request emitted by the right-click menu in the graph.
    fn handle_macro_request(&mut self, req: MacroRequest) {
        match req {
            MacroRequest::SaveAs { node_id, subgraph_path } => {
                // The selected subgraph's parent level is the current
                // subgraph_path; the inner graph we want to save belongs to
                // the target node itself.
                let _ = node_id;
                self.save_macro_dialog = Some(SaveMacroDialog {
                    target_node: node_id,
                    parent_path: subgraph_path,
                    name: String::new(),
                    group: String::new(),
                    description: String::new(),
                    tags: String::new(),
                    error: None,
                    overwrite_path: None,
                });
            }
        }
    }

    /// Validate dialog input, build the .lbm file, write it. Returns Err on
    /// failure (kept open in the dialog).
    fn save_pending_macro(&mut self) -> Result<(), String> {
        let dlg = self.save_macro_dialog.as_ref()
            .ok_or_else(|| "no dialog state".to_string())?;
        let target_id = dlg.target_node;
        let group = dlg.group.trim().trim_matches('/').to_string();
        let name = dlg.name.trim().to_string();
        if name.is_empty() {
            return Err("Name is required".to_string());
        }
        let tags: Vec<String> = dlg.tags.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        // Find the inner graph for the target subgraph.
        let level = self.graph.find_level_for_subgraph(target_id)
            .ok_or_else(|| "subgraph has no inner level (yet)".to_string())?;
        let inner = project::save_level(level, &self.graph);

        // Capture the subgraph's external port defs so the macro can
        // re-instantiate with the same I/O shape.
        let (inputs, outputs) = self.subgraph_port_defs(target_id, &dlg.parent_path);
        let size = self.subgraph_size(target_id);

        let m = Macro {
            format_version: macros::MACRO_FORMAT_VERSION,
            name: name.clone(),
            creator: String::new(),
            date: macros::now_timestamp(),
            description: dlg.description.clone(),
            tags,
            inputs,
            outputs,
            size,
            graph: inner,
        };
        let path = match &dlg.overwrite_path {
            Some(p) => p.clone(),
            None => {
                let p = self.library.path_for(&group, &name);
                if p.exists() {
                    return Err(format!("File already exists: {}", p.display()));
                }
                p
            }
        };
        m.save_to_file(&path)?;
        self.library.rescan();
        Ok(())
    }

    /// Read the subgraph's external port defs out of its widget at the
    /// given parent path. Returns `(inputs, outputs)` — empty if not found.
    fn subgraph_port_defs(
        &self,
        target_id: NodeId,
        _parent_path: &[NodeId],
    ) -> (
        Vec<engine::nodes::meta::subgraph::SubgraphPortDef>,
        Vec<engine::nodes::meta::subgraph::SubgraphPortDef>,
    ) {
        // The macro's port defs come from the `find_level_for_subgraph` we
        // already used for the inner graph: the bridge nodes inside that
        // level mirror the subgraph's external ports.
        // But simpler: walk up to the parent level and read SubgraphWidget
        // directly. We rely on the SubgraphWidget being in the active
        // graph somewhere with the matching node id.
        for level in self.graph.all_levels() {
            for (i, n) in level.nodes.iter().enumerate() {
                if level.states[i].id != target_id { continue; }
                // Can't downcast through &dyn; need &mut. Use a different
                // route: serialize the subgraph's pending shared.save_data
                // which contains the port defs JSON.
                let shared = n.shared_state().lock().unwrap();
                if let Some(data) = &shared.save_data {
                    use engine::nodes::meta::subgraph::SubgraphPortDef;
                    let inputs = data.get("inputs")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                            .collect())
                        .unwrap_or_default();
                    let outputs = data.get("outputs")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|v| serde_json::from_value::<SubgraphPortDef>(v.clone()).ok())
                            .collect())
                        .unwrap_or_default();
                    return (inputs, outputs);
                }
            }
        }
        (Vec::new(), Vec::new())
    }

    /// Walk every level looking for a Subgraph node with the given id and
    /// return its `size_override` (the user-resized size). Returns None when
    /// the user has never resized it.
    fn subgraph_size(&self, target_id: NodeId) -> Option<[f32; 2]> {
        for level in self.graph.all_levels() {
            for (i, _n) in level.nodes.iter().enumerate() {
                if level.states[i].id != target_id { continue; }
                return level.states[i].size_override.map(|s| [s.x, s.y]);
            }
        }
        None
    }

    /// Load a macro file and add it as a locked Subgraph at the given
    /// world-space position. All NodeIds in the inner graph are remapped to
    /// fresh ones to avoid collisions with existing project nodes.
    fn instantiate_macro_from_path(&mut self, path: &std::path::Path, canvas_pos: egui::Pos2) {
        let m = match Macro::load_from_file(path) {
            Ok(m) => m,
            Err(e) => { eprintln!("load macro: {}", e); return; }
        };
        if m.format_version != macros::MACRO_FORMAT_VERSION {
            eprintln!("macro '{}': unsupported format_version {}", m.name, m.format_version);
        }

        // Remap all inner-graph NodeIds (recursively into nested subgraphs)
        // to fresh ones from the graph's allocator.
        let mut inner = m.graph;
        remap_project_ids(&mut inner, || self.graph.alloc_id());

        let new_id = self.graph.alloc_id();
        let data = serde_json::json!({
            "name": m.name,
            "inputs": m.inputs,
            "outputs": m.outputs,
            "locked": true,
            "macro_description": m.description,
            "macro_path": path.display().to_string(),
        });
        let saved = project::SavedNode {
            type_name: "Subgraph".to_string(),
            id: new_id.0,
            pos: [canvas_pos.x, canvas_pos.y],
            size: m.size,
            params: Vec::new(),
            data: Some(data),
            inner_graph: Some(inner),
        };

        // Load this single node into the active level via the existing
        // project loader machinery (handles inner-graph descent + bridges).
        let pf = project::ProjectFile { nodes: vec![saved.clone()], connections: Vec::new() };
        let _indices = project::load_graph(&mut self.graph, &pf);

        // Spawn engine nodes + send queued commands + load_data, same as
        // load_project_from / undo apply_project does.
        self.wire_new_nodes();
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }
        // The macro's nodes need their per-node data restored too (inner
        // subgraphs have data fields, etc.).
        self.send_load_data_recursive(&pf.nodes, &_indices, vec![]);
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
                    if ui.button("New").clicked() {
                        ui.close_menu();
                        self.new_project();
                    }
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
                    if ui.checkbox(&mut self.show_input_controllers, "Input Controllers").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_audio_inputs, "Audio Inputs").changed() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.show_library, "Macro Library").changed() {
                        ui.close_menu();
                    }
                    ui.separator();
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

        // Input Controllers window.
        if self.show_input_controllers {
            let r = egui::Window::new("Input Controllers")
                .open(&mut self.show_input_controllers)
                .default_size([400.0, 500.0])
                .show(ctx, |ui| {
                    widgets::input_controller_list::show(ui, &mut self.input_controllers);
                });
            mark_hovered(r);
        }

        // Audio Inputs window.
        if self.show_audio_inputs {
            let r = egui::Window::new("Audio Inputs")
                .open(&mut self.show_audio_inputs)
                .default_size([420.0, 500.0])
                .show(ctx, |ui| {
                    widgets::audio_input_list::show(ui, &mut self.audio_inputs);
                });
            mark_hovered(r);
        }
        // Periodic reconnect / port availability checks.
        self.input_controllers.tick_reconnect();
        self.audio_inputs.tick_reconnect();

        // Library side panel (left).
        if self.show_library {
            egui::SidePanel::left("library_panel")
                .default_width(220.0)
                .show(ctx, |ui| {
                    self.show_library_panel(ui);
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
        let has_selection = !self.graph.selected_nodes_mut().is_empty();
        let show_inspector = match self.config.inspector_mode {
            InspectorMode::Show => true,
            InspectorMode::Hide => false,
            InspectorMode::Auto => has_selection,
        };
        if show_inspector {
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

        // Accept a macro drop on the canvas. We only consume the payload
        // when the pointer is released inside the canvas rect — otherwise
        // egui keeps the drag alive and the user can continue aiming.
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some(pos) = ctx.pointer_interact_pos() {
                if self.graph.canvas_rect().contains(pos) {
                    if let Some(payload) = egui::DragAndDrop::take_payload::<MacroDragPayload>(ctx) {
                        let world = self.graph.screen_to_world(pos);
                        self.instantiate_macro_from_path(&payload.path, world);
                    }
                }
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

/// Walk a `ProjectFile` (recursively into nested subgraphs) and rewrite
/// every node id + connection endpoint to a fresh id allocated via `alloc`.
/// Bridge sentinel ids (`u64::MAX` / `u64::MAX-1`) are left intact — they're
/// position-based, not identity-based.
fn remap_project_ids<F: FnMut() -> NodeId>(pf: &mut project::ProjectFile, mut alloc: F) {
    use std::collections::HashMap;
    use engine::nodes::meta::subgraph::{BRIDGE_IN_NODE_ID, BRIDGE_OUT_NODE_ID};

    fn is_bridge(id: u64) -> bool {
        id == BRIDGE_IN_NODE_ID.0 || id == BRIDGE_OUT_NODE_ID.0
    }

    fn walk<F: FnMut() -> NodeId>(pf: &mut project::ProjectFile, alloc: &mut F) {
        let mut map: HashMap<u64, u64> = HashMap::new();
        for n in &mut pf.nodes {
            if is_bridge(n.id) { continue; }
            let new = alloc().0;
            map.insert(n.id, new);
            n.id = new;
        }
        for c in &mut pf.connections {
            if let Some(&n) = map.get(&c.from_node) { c.from_node = n; }
            if let Some(&n) = map.get(&c.to_node) { c.to_node = n; }
        }
        for n in &mut pf.nodes {
            if let Some(inner) = n.inner_graph.as_mut() {
                walk(inner, alloc);
            }
        }
    }
    walk(pf, &mut alloc);
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
