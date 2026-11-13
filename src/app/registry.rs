use crate::LightBeatApp;
use crate::engine;
use crate::widgets;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use crate::beat_clock::BeatListener;
use crate::engine::types::{NodeId, SharedState};
use crate::widgets::nodes::node::NodeWidget;
use crate::beat_clock::BeatPattern;
use crate::engine::types::{new_shared_state, EngineCommand, PortType, ProcessNode, SubgraphInnerCmd};
use crate::engine::nodes::display::color_display::ColorDisplayProcessNode;
use crate::engine::nodes::display::scope::ScopeProcessNode;
use crate::engine::nodes::display::led_display::LedDisplayProcessNode;
use crate::engine::nodes::display::value_display::ValueDisplayProcessNode;
use crate::engine::nodes::io::audio_input::AudioInputProcessNode;
use crate::engine::nodes::io::clock::ClockProcessNode;
use crate::engine::nodes::io::dmx_input::DmxInputProcessNode;
use crate::engine::nodes::io::input_controller::InputControllerProcessNode;
use crate::engine::nodes::io::midi_input::MidiInputProcessNode;
use crate::engine::nodes::io::midi_output::MidiOutputProcessNode;
use crate::engine::nodes::io::push1::Push1ProcessNode;
use crate::engine::nodes::io::launchpad::LaunchpadProcessNode;
use crate::engine::nodes::io::x1::X1ProcessNode;
use crate::engine::nodes::io::internal_clock::InternalClockProcessNode;
use crate::engine::nodes::ui::button::ButtonProcessNode;
use crate::engine::nodes::ui::button_group::ButtonGroupProcessNode;
use crate::engine::nodes::ui::fader::FaderProcessNode;
use crate::engine::nodes::ui::fader_group::FaderGroupProcessNode;
use crate::engine::nodes::ui::peak_meter::PeakMeterProcessNode;
use crate::engine::nodes::ui::color_mixer::ColorMixerProcessNode;
use crate::engine::nodes::ui::xy_pad::XyPadProcessNode;
use crate::engine::nodes::ui::gradient_stops::GradientStopsProcessNode;
use crate::engine::nodes::math::bipolar::BipolarProcessNode;
use crate::engine::nodes::math::scale_offset::ScaleOffsetProcessNode;
use crate::engine::nodes::math::change_detect::ChangeDetectProcessNode;
use crate::engine::nodes::math::schmitt::SchmittTriggerProcessNode;
use crate::engine::nodes::math::toggle_bank::ToggleBankProcessNode;
use crate::engine::nodes::math::gain::GainProcessNode;
use crate::engine::nodes::math::phase_accumulator::PhaseAccumulatorProcessNode;
use crate::engine::nodes::math::port_hub::PortHubProcessNode;
use crate::engine::nodes::math::trigger_bank::TriggerBankProcessNode;
use crate::engine::nodes::math::flipflop::{FlipFlopProcessNode, JkFlipFlopProcessNode};
use crate::engine::nodes::math::color_modifier::ColorModifierProcessNode;
use crate::engine::nodes::math::color_components::{ColorToComponentsProcessNode, ComponentsToColorProcessNode};
use crate::engine::nodes::math::color_reshape::{ColorReshapeProcessNode, ReshapeKind};
use crate::engine::nodes::math::compare::{CompareOp, CompareProcessNode};
use crate::engine::nodes::math::constant::ConstantProcessNode;
use crate::engine::nodes::math::counter::CounterProcessNode;
use crate::engine::nodes::math::gradient_source::GradientSourceProcessNode;
use crate::engine::nodes::math::lookup::LookupProcessNode;
use crate::engine::nodes::math::logic_gate::{LogicOp, LogicGateProcessNode};
use crate::engine::nodes::math::math_op::{MathOp, MathProcessNode};
use crate::engine::nodes::math::multiplex::{DemultiplexerProcessNode, MultiplexerProcessNode};
use crate::engine::nodes::math::palette_select::PaletteSelectProcessNode;
use crate::engine::nodes::math::palette_to_gradient::PaletteToGradientProcessNode;
use crate::engine::nodes::meta::subgraph::SubgraphProcessNode;
use crate::engine::nodes::math::scaler::ScalerProcessNode;
use crate::engine::nodes::math::oscillator::{OscFunc, OscillatorProcessNode};
use crate::engine::nodes::math::position_ops::{PositionMergeProcessNode, PositionSplitProcessNode};
use crate::engine::effects::bar::Bar;
use crate::engine::effects::fill::Fill;
use crate::engine::effects::node::EffectNode;
use crate::engine::effects::ripple::Ripple;
use crate::engine::effects::spark::SparkBurst;
use crate::engine::effects::stack::EffectStackProcessNode;
use crate::engine::effects::wave::Wave;
use crate::engine::nodes::transport::clock_divider::ClockDividerProcessNode;
use crate::engine::nodes::transport::clock_gen::ClockGenProcessNode;
use crate::engine::nodes::transport::delay::TriggerDelayProcessNode;
use crate::engine::nodes::transport::envelope::EnvelopeProcessNode;
use crate::engine::nodes::transport::hold::TriggerHoldProcessNode;
use crate::engine::nodes::transport::latch::LatchProcessNode;
use crate::engine::nodes::display::meter::{MeterKind, MeterProcessNode};
use crate::engine::nodes::math::formula::FormulaProcessNode;
use crate::engine::nodes::math::random::RandomProcessNode;
use crate::engine::nodes::transport::sample_hold::SampleHoldProcessNode;
use crate::engine::nodes::transport::smoothing::SmoothingProcessNode;
use crate::engine::nodes::transport::time_to_phase::TimeToPhaseProcessNode;
use crate::engine::nodes::transport::transition::TransitionProcessNode;
use crate::engine::nodes::transport::lfo::LfoProcessNode;
use crate::engine::nodes::transport::phase_scaler::PhaseScalerProcessNode;
use crate::engine::nodes::transport::step_sequencer::StepSequencerProcessNode;
use crate::widgets::nodes::display::color_display::ColorDisplayWidget;
use crate::widgets::nodes::display::scope::ScopeWidget;
use crate::widgets::nodes::display::led_display::LedDisplayWidget;
use crate::widgets::nodes::display::value_display::ValueDisplayWidget;
use crate::widgets::nodes::io::audio_input::AudioInputWidget;
use crate::widgets::nodes::io::clock::ClockWidget;
use crate::widgets::nodes::io::dmx_input::DmxInputWidget;
use crate::widgets::nodes::io::input_controller::InputControllerWidget;
use crate::widgets::nodes::io::midi_input::MidiInputWidget;
use crate::widgets::nodes::io::midi_output::MidiOutputWidget;
use crate::widgets::nodes::io::push1::Push1Widget;
use crate::widgets::nodes::io::launchpad::LaunchpadWidget;
use crate::widgets::nodes::io::x1::X1Widget;
use crate::widgets::nodes::io::internal_clock::InternalClockWidget;
use crate::widgets::nodes::ui::button::ButtonWidget;
use crate::widgets::nodes::ui::button_group::ButtonGroupWidget;
use crate::widgets::nodes::ui::fader::FaderWidget;
use crate::widgets::nodes::ui::fader_group::FaderGroupWidget;
use crate::widgets::nodes::ui::peak_meter::PeakMeterWidget;
use crate::widgets::nodes::ui::color_mixer::ColorMixerWidget;
use crate::widgets::nodes::ui::xy_pad::XyPadWidget;
use crate::widgets::nodes::ui::gradient_stops::GradientStopsWidget;
use crate::widgets::nodes::math::bipolar::BipolarWidget;
use crate::widgets::nodes::math::scale_offset::ScaleOffsetWidget;
use crate::widgets::nodes::math::change_detect::ChangeDetectWidget;
use crate::widgets::nodes::math::schmitt::SchmittTriggerWidget;
use crate::widgets::nodes::math::toggle_bank::ToggleBankWidget;
use crate::widgets::nodes::math::gain::GainWidget;
use crate::widgets::nodes::math::phase_accumulator::PhaseAccumulatorWidget;
use crate::widgets::nodes::math::port_hub::PortHubWidget;
use crate::widgets::nodes::math::trigger_bank::TriggerBankWidget;
use crate::widgets::nodes::math::flipflop::{FlipFlopKind, FlipFlopWidget};
use crate::widgets::nodes::math::color_modifier::ColorModifierWidget;
use crate::widgets::nodes::math::color_components::{ColorToComponentsWidget, ComponentsToColorWidget};
use crate::widgets::nodes::math::color_reshape::ColorReshapeWidget;
use crate::widgets::nodes::math::compare::CompareWidget;
use crate::widgets::nodes::math::constant::ConstantWidget;
use crate::widgets::nodes::math::counter::CounterWidget;
use crate::widgets::nodes::math::gradient_source::GradientSourceWidget;
use crate::widgets::nodes::math::lookup::LookupWidget;
use crate::widgets::nodes::math::logic_gate::LogicGateWidget;
use crate::widgets::nodes::math::math_op::MathWidget;
use crate::widgets::nodes::math::multiplex::{DemultiplexerWidget, MultiplexerWidget};
use crate::widgets::nodes::math::palette_select::PaletteSelectWidget;
use crate::widgets::nodes::math::palette_to_gradient::PaletteToGradientWidget;
use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
use crate::widgets::nodes::math::scaler::ScalerWidget;
use crate::widgets::nodes::math::oscillator::OscillatorWidget;
use crate::widgets::nodes::math::position_ops::{PositionMergeWidget, PositionSplitWidget};
use crate::widgets::nodes::output::effect_stack::EffectStackWidget;
use crate::widgets::nodes::transport::clock_divider::ClockDividerWidget;
use crate::widgets::nodes::transport::clock_gen::ClockGenWidget;
use crate::widgets::nodes::transport::delay::TriggerDelayWidget;
use crate::widgets::nodes::transport::envelope::EnvelopeWidget;
use crate::widgets::nodes::transport::hold::TriggerHoldWidget;
use crate::widgets::nodes::transport::latch::LatchWidget;
use crate::widgets::nodes::display::meter::MeterWidget;
use crate::widgets::nodes::math::formula::FormulaWidget;
use crate::widgets::nodes::math::random::RandomWidget;
use crate::widgets::nodes::transport::sample_hold::SampleHoldWidget;
use crate::widgets::nodes::transport::smoothing::SmoothingWidget;
use crate::widgets::nodes::transport::time_to_phase::TimeToPhaseWidget;
use crate::widgets::nodes::transport::transition::TransitionWidget;
use crate::widgets::nodes::transport::lfo::LfoWidget;
use crate::widgets::nodes::transport::phase_scaler::PhaseScalerWidget;
use crate::widgets::nodes::transport::step_sequencer::StepSequencerWidget;

/// What the constructors of a node's two halves may need from the app: the
/// hubs and registries the nodes share. Every field is a handle, so cloning
/// the context is cheap.
#[derive(Clone)]
pub(crate) struct NodeContext {
    pub snapshot: Arc<parking_lot::Mutex<crate::beat_clock::LinkSnapshot>>,
    pub input_controllers: crate::input_controller::SharedControllers,
    pub audio_inputs: crate::audio::manager::SharedAudioInputs,
    pub dmx_inputs: crate::dmx_input::DmxInputHub,
    pub midi_hub: crate::midi_io::MidiHub,
    pub midi_learned: widgets::nodes::io::midi_common::SharedLearned,
    pub gradient_library: widgets::nodes::math::gradient_source::SharedGradientLibrary,
    pub palette_ctx: widgets::nodes::math::palette_select::SharedPaletteContext,
    pub group_ctx: widgets::group_context::SharedGroupContext,
    pub stack_names: widgets::nodes::output::effect::SharedStackNames,
    pub portals: engine::nodes::meta::portal::SharedPortalRegistry,
    pub effect_hub: engine::effects::EffectHub,
    pub object_store: crate::dmx_io::SharedObjectStore,
    /// Beat subscriptions an engine constructor asked for. The beat clock is
    /// not a handle that can be cloned, so the app registers them afterwards.
    wanted_beats: Rc<RefCell<Vec<(BeatPattern, Arc<parking_lot::Mutex<dyn BeatListener>>)>>>,
}

impl NodeContext {
    pub fn subscribe_beat(&self, pattern: BeatPattern, listener: Arc<parking_lot::Mutex<dyn BeatListener>>) {
        self.wanted_beats.borrow_mut().push((pattern, listener));
    }

    fn take_wanted_beats(&self) -> Vec<(BeatPattern, Arc<parking_lot::Mutex<dyn BeatListener>>)> {
        std::mem::take(&mut *self.wanted_beats.borrow_mut())
    }
}

/// One node type: where it sits in the menus, how many channels its shared
/// state is sized for, and how to make its two halves. A node cannot be
/// registered on one side only, and both halves are found by the same name.
pub(crate) struct NodeDescriptor {
    pub category: &'static str,
    pub name: &'static str,
    /// Channels of the shared state (inputs, outputs): the worst case the
    /// node can grow to, not what it starts with.
    pub channels: (usize, usize),
    pub widget: Box<dyn Fn(NodeId, SharedState, &NodeContext) -> Box<dyn NodeWidget>>,
    pub engine: Box<dyn Fn(NodeId, &NodeContext) -> Box<dyn ProcessNode>>,
}

fn node(
    category: &'static str,
    name: &'static str,
    channels: (usize, usize),
    widget: impl Fn(NodeId, SharedState, &NodeContext) -> Box<dyn NodeWidget> + 'static,
    engine: impl Fn(NodeId, &NodeContext) -> Box<dyn ProcessNode> + 'static,
) -> NodeDescriptor {
    NodeDescriptor { category, name, channels, widget: Box::new(widget), engine: Box::new(engine) }
}

/// The groups of the Add Nodes panel, the canvas menus and quick add (which
/// addresses a node as `<group lowercased>/<name>`), in panel order.
pub(crate) const CATEGORIES: [&str; 14] = [
    "Input", "Output", "Clock", "Signal", "Trigger", "Math", "Compare", "Logic",
    "Color", "Position", "Route", "Portal", "Display", "Effect",
];

/// Every node type, grouped in the order of `CATEGORIES`.
pub(crate) fn node_table() -> Vec<NodeDescriptor> {
    let mut table = Vec::new();
    table.push(node(
        "Clock", "Link Clock", (0, 5),
        |id, s, _| Box::new(ClockWidget::new(id, s)),
        |id, ctx| {
            let node = ClockProcessNode::new(id, Arc::clone(&ctx.snapshot));
            ctx.subscribe_beat(BeatPattern::every(1), node.beat_state.clone());
            Box::new(node)
        },
    ));
    // 4 inputs (play/stop, bpm, set bpm, reset) and 3 outputs.
    table.push(node(
        "Clock", "Internal Clock", (4, 3),
        |id, s, _| Box::new(InternalClockWidget::new(id, s)),
        |id, _| Box::new(InternalClockProcessNode::new(id)),
    ));
    // Generous channel budget on both sides — the engine resizes its
    // active port list as the bound controller's layout changes. 192
    // covers generic MIDI and BCF2000 with headroom; Push 1 uses its
    // own dedicated node.
    table.push(node(
        "Input", "Input Controller", (192, 192),
        |id, s, ctx| Box::new(InputControllerWidget::new(id, s, ctx.input_controllers.clone())),
        |id, ctx| Box::new(InputControllerProcessNode::new(id, ctx.input_controllers.clone())),
    ));
    // The status port and up to 64 entries of at most three channels.
    table.push(node(
        "Input", "DMX Input", (0, 200),
        |id, s, ctx| Box::new(DmxInputWidget::new(id, s, ctx.dmx_inputs.clone())),
        |id, ctx| Box::new(DmxInputProcessNode::new(id, ctx.dmx_inputs.clone())),
    ));
    // The status port and up to 32 controls.
    table.push(node(
        "Input", "MIDI Input", (0, 33),
        |id, s, ctx| Box::new(MidiInputWidget::new(id, s, ctx.midi_hub.clone(), ctx.midi_learned.clone())),
        |id, ctx| Box::new(MidiInputProcessNode::new(id, ctx.midi_hub.clone())),
    ));
    table.push(node(
        "Output", "MIDI Output", (32, 1),
        |id, s, ctx| Box::new(MidiOutputWidget::new(id, s, ctx.midi_hub.clone(), ctx.midi_learned.clone())),
        |id, ctx| Box::new(MidiOutputProcessNode::new(id, ctx.midi_hub.clone())),
    ));
    // 26 outputs (incl. "connected"), 21 inputs (see push1.rs). 32/32 gives headroom.
    table.push(node(
        "Input", "Push 1", (32, 32),
        |id, s, ctx| Box::new(Push1Widget::new(id, s, ctx.input_controllers.clone())),
        |id, ctx| Box::new(Push1ProcessNode::new(id, ctx.input_controllers.clone())),
    ));
    // 34 button outputs + 4 encoder outputs + 8 pot outputs + "any change" +
    // "connected" = 48 outputs. 34 LED feedback inputs. 56/56 leaves headroom.
    table.push(node(
        "Input", "X1", (56, 56),
        |id, s, ctx| Box::new(X1Widget::new(id, s, ctx.input_controllers.clone())),
        |id, ctx| Box::new(X1ProcessNode::new(id, ctx.input_controllers.clone())),
    ));
    // 82 outputs = 64 pads + 8 side + 8 top + "any change" + "connected".
    // 80 LED feedback inputs, each `Color` (3 channels) = 240
    // channels. 256 gives a bit of headroom.
    table.push(node(
        "Input", "Launchpad S", (256, 82),
        |id, s, ctx| Box::new(LaunchpadWidget::new(id, s, ctx.input_controllers.clone())),
        |id, ctx| Box::new(LaunchpadProcessNode::new(id, ctx.input_controllers.clone())),
    ));
    table.push(node(
        "Input", "Audio Input", (0, 32),
        |id, s, ctx| Box::new(AudioInputWidget::new(id, s, ctx.audio_inputs.clone())),
        |id, ctx| Box::new(AudioInputProcessNode::new(id, ctx.audio_inputs.clone())),
    ));
    // 1 optional input (Logic), 1 output.
    table.push(node(
        "Input", "Button", (1, 1),
        |id, s, _| Box::new(ButtonWidget::new(id, s)),
        |id, _| Box::new(ButtonProcessNode::new(id)),
    ));
    // 1 optional input (when "Enable input" is checked), 1 output.
    table.push(node(
        "Input", "Fader", (1, 1),
        |id, s, _| Box::new(FaderWidget::new(id, s)),
        |id, _| Box::new(FaderProcessNode::new(id)),
    ));
    table.push(node(
        "Input", "Button Group", (256, 256),
        |id, s, _| Box::new(ButtonGroupWidget::new(id, s)),
        |id, _| Box::new(ButtonGroupProcessNode::new(id)),
    ));
    table.push(node(
        "Input", "Fader Group", (256, 256),
        |id, s, _| Box::new(FaderGroupWidget::new(id, s)),
        |id, _| Box::new(FaderGroupProcessNode::new(id)),
    ));
    // No inputs; outputs x and y (0..1).
    table.push(node(
        "Input", "XY Pad", (0, 2),
        |id, s, _| Box::new(XyPadWidget::new(id, s)),
        |id, _| Box::new(XyPadProcessNode::new(id)),
    ));
    table.push(node(
        "Input", "Color Mixer", (0, 3),
        |id, s, _| Box::new(ColorMixerWidget::new(id, s)),
        |id, _| Box::new(ColorMixerProcessNode::new(id)),
    ));
    // 1 Palette input (12 channels) for the live preview, 4 position
    // outputs to feed Palette → Gradient's pos1..pos4.
    table.push(node(
        "Input", "Gradient Stops", (12, 4),
        |id, s, _| Box::new(GradientStopsWidget::new(id, s)),
        |id, _| Box::new(GradientStopsProcessNode::new(id)),
    ));
    table.push(node(
        "Display", "Peak Level Meter", (2, 0),
        |id, s, _| Box::new(PeakMeterWidget::new(id, s)),
        |id, _| Box::new(PeakMeterProcessNode::new(id)),
    ));
    table.push(node(
        "Clock", "Phase Scaler", (4, 16),
        |id, s, _| Box::new(PhaseScalerWidget::new(id, s)),
        |id, _| Box::new(PhaseScalerProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "LFO", (1, 2),
        |id, s, _| Box::new(LfoWidget::new(id, s)),
        |id, _| Box::new(LfoProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Step Sequencer", (2, 3),
        |id, s, _| Box::new(StepSequencerWidget::new(id, s)),
        |id, _| Box::new(StepSequencerProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Envelope", (2, 2),
        |id, s, _| Box::new(EnvelopeWidget::new(id, s)),
        |id, _| Box::new(EnvelopeProcessNode::new(id)),
    ));
    table.push(node(
        "Trigger", "Trigger Delay", (2, 1),
        |id, s, _| Box::new(TriggerDelayWidget::new(id, s)),
        |id, _| Box::new(TriggerDelayProcessNode::new(id)),
    ));
    table.push(node(
        "Trigger", "Trigger Hold", (2, 1),
        |id, s, _| Box::new(TriggerHoldWidget::new(id, s)),
        |id, _| Box::new(TriggerHoldProcessNode::new(id)),
    ));
    table.push(node(
        "Math", "Formula", (1, 1),
        |id, s, _| Box::new(FormulaWidget::new(id, s)),
        |id, _| Box::new(FormulaProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Random", (1, 1),
        |id, s, _| Box::new(RandomWidget::new(id, s)),
        |id, _| Box::new(RandomProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Sample & Hold", (41, 40),
        |id, s, _| Box::new(SampleHoldWidget::new(id, s)),
        |id, _| Box::new(SampleHoldProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Smoothing", (40, 40),
        |id, s, _| Box::new(SmoothingWidget::new(id, s)),
        |id, _| Box::new(SmoothingProcessNode::new(id)),
    ));
    table.push(node(
        "Trigger", "Tick Delay", (40, 40),
        |id, s, _| Box::new(LatchWidget::new(id, s)),
        |id, _| Box::new(LatchProcessNode::new(id)),
    ));
    table.push(node(
        "Clock", "Clock Divider", (1, 16),
        |id, s, _| Box::new(ClockDividerWidget::new(id, s)),
        |id, _| Box::new(ClockDividerProcessNode::new(id)),
    ));
    table.push(node(
        "Clock", "Clock Gen", (1, 1),
        |id, s, _| Box::new(ClockGenWidget::new(id, s)),
        |id, _| Box::new(ClockGenProcessNode::new(id)),
    ));
    table.push(node(
        "Clock", "Time to Phase", (1, 2),
        |id, s, _| Box::new(TimeToPhaseWidget::new(id, s)),
        |id, _| Box::new(TimeToPhaseProcessNode::new(id)),
    ));
    // trigger(1) + phase(1) + palette(12) = 14 input channels, 12 output channels max
    table.push(node(
        "Signal", "Transition", (42, 40),
        |id, s, _| Box::new(TransitionWidget::new(id, s)),
        |id, _| Box::new(TransitionProcessNode::new(id)),
    ));
    table.push(node(
        "Math", "Add", (16, 1),
        |id, s, _| Box::new(MathWidget::new(id, MathOp::Add, s)),
        |id, _| Box::new(MathProcessNode::new(id, MathOp::Add)),
    ));
    table.push(node(
        "Math", "Sub", (16, 1),
        |id, s, _| Box::new(MathWidget::new(id, MathOp::Sub, s)),
        |id, _| Box::new(MathProcessNode::new(id, MathOp::Sub)),
    ));
    table.push(node(
        "Math", "Mul", (16, 1),
        |id, s, _| Box::new(MathWidget::new(id, MathOp::Mul, s)),
        |id, _| Box::new(MathProcessNode::new(id, MathOp::Mul)),
    ));
    table.push(node(
        "Math", "Div", (16, 1),
        |id, s, _| Box::new(MathWidget::new(id, MathOp::Div, s)),
        |id, _| Box::new(MathProcessNode::new(id, MathOp::Div)),
    ));
    table.push(node(
        "Math", "Sin", (2, 1),
        |id, s, _| Box::new(OscillatorWidget::new(id, OscFunc::Sin, s)),
        |id, _| Box::new(OscillatorProcessNode::new(id, OscFunc::Sin)),
    ));
    table.push(node(
        "Math", "Cos", (2, 1),
        |id, s, _| Box::new(OscillatorWidget::new(id, OscFunc::Cos, s)),
        |id, _| Box::new(OscillatorProcessNode::new(id, OscFunc::Cos)),
    ));
    table.push(node(
        "Color", "Components to Color", (4, 4),
        |id, s, _| Box::new(ComponentsToColorWidget::new(id, s)),
        |id, _| Box::new(ComponentsToColorProcessNode::new(id)),
    ));
    table.push(node(
        "Color", "Color to Components", (4, 4),
        |id, s, _| Box::new(ColorToComponentsWidget::new(id, s)),
        |id, _| Box::new(ColorToComponentsProcessNode::new(id)),
    ));
    for kind in ReshapeKind::ALL {
        table.push(node(
            "Color", kind.type_name(), (48, 48),
            // Room for a full gradient (40 channels) on either side.
            move |id, s, _| Box::new(ColorReshapeWidget::new(id, s, kind)),
            move |id, _| Box::new(ColorReshapeProcessNode::new(id, kind)),
        ));
    }
    // Outputs: 1 (rows) + worst case 8 columns × Gradient (40 ch) = 321.
    table.push(node(
        "Route", "Lookup Table", (1, 321),
        |id, s, ctx| Box::new(LookupWidget::new(id, s, ctx.gradient_library.clone())),
        |id, _| Box::new(LookupProcessNode::new(id)),
    ));
    table.push(node(
        "Signal", "Const Value", (0, 1),
        |id, s, _| Box::new(ConstantWidget::new(id, PortType::Untyped, s)),
        |id, _| Box::new(ConstantProcessNode::new(id, PortType::Untyped, 0.0)),
    ));
    table.push(node(
        "Signal", "Const Logic", (0, 1),
        |id, s, _| Box::new(ConstantWidget::new(id, PortType::Logic, s)),
        |id, _| Box::new(ConstantProcessNode::new(id, PortType::Logic, 0.0)),
    ));
    table.push(node(
        "Signal", "Const Phase", (0, 1),
        |id, s, _| Box::new(ConstantWidget::new(id, PortType::Phase, s)),
        |id, _| Box::new(ConstantProcessNode::new(id, PortType::Phase, 0.0)),
    ));
    // Inputs: trigger, reset, max (3 channels). Outputs: count, wrap (2).
    table.push(node(
        "Trigger", "Counter", (3, 2),
        |id, s, _| Box::new(CounterWidget::new(id, s)),
        |id, _| Box::new(CounterProcessNode::new(id)),
    ));
    table.push(node(
        "Trigger", "Change Detect", (2, 2),
        |id, s, _| Box::new(ChangeDetectWidget::new(id, s)),
        |id, _| Box::new(ChangeDetectProcessNode::new(id)),
    ));
    table.push(node(
        "Trigger", "Schmitt Trigger", (1, 1),
        |id, s, _| Box::new(SchmittTriggerWidget::new(id, s)),
        |id, _| Box::new(SchmittTriggerProcessNode::new(id)),
    ));
    // Up to 16 channels, each Logic in and out.
    table.push(node(
        "Trigger", "Toggle Bank", (16, 16),
        |id, s, _| Box::new(ToggleBankWidget::new(id, s)),
        |id, _| Box::new(ToggleBankProcessNode::new(id)),
    ));
    // Up to 16 channels, each Untyped in and Logic out.
    table.push(node(
        "Trigger", "Trigger Bank", (16, 16),
        |id, s, _| Box::new(TriggerBankWidget::new(id, s)),
        |id, _| Box::new(TriggerBankProcessNode::new(id)),
    ));
    table.push(node(
        "Math", "Gain", (1, 1),
        |id, s, _| Box::new(GainWidget::new(id, s)),
        |id, _| Box::new(GainProcessNode::new(id)),
    ));
    table.push(node(
        "Clock", "Phase Accumulator", (2, 1),
        |id, s, _| Box::new(PhaseAccumulatorWidget::new(id, s)),
        |id, _| Box::new(PhaseAccumulatorProcessNode::new(id)),
    ));
    // Max channels = MUX_MAX_SLOTS (16) × max channels per type
    // (Gradient = 40) = 640. Round up to 1024 for headroom.
    table.push(node(
        "Route", "Port Hub", (1024, 1024),
        |id, s, _| Box::new(PortHubWidget::new(id, s)),
        |id, _| Box::new(PortHubProcessNode::new(id)),
    ));
    table.push(node(
        "Compare", ">=", (2, 1),
        |id, s, _| Box::new(CompareWidget::new(id, CompareOp::Gte, s)),
        |id, _| Box::new(CompareProcessNode::new(id, CompareOp::Gte)),
    ));
    table.push(node(
        "Compare", "<=", (2, 1),
        |id, s, _| Box::new(CompareWidget::new(id, CompareOp::Lte, s)),
        |id, _| Box::new(CompareProcessNode::new(id, CompareOp::Lte)),
    ));
    table.push(node(
        "Compare", "==", (2, 1),
        |id, s, _| Box::new(CompareWidget::new(id, CompareOp::Eq, s)),
        |id, _| Box::new(CompareProcessNode::new(id, CompareOp::Eq)),
    ));
    table.push(node(
        "Compare", "!=", (2, 1),
        |id, s, _| Box::new(CompareWidget::new(id, CompareOp::Neq, s)),
        |id, _| Box::new(CompareProcessNode::new(id, CompareOp::Neq)),
    ));
    table.push(node(
        "Logic", "AND", (16, 1),
        |id, s, _| Box::new(LogicGateWidget::new(id, LogicOp::And, s)),
        |id, _| Box::new(LogicGateProcessNode::new(id, LogicOp::And)),
    ));
    table.push(node(
        "Logic", "OR", (16, 1),
        |id, s, _| Box::new(LogicGateWidget::new(id, LogicOp::Or, s)),
        |id, _| Box::new(LogicGateProcessNode::new(id, LogicOp::Or)),
    ));
    table.push(node(
        "Logic", "XOR", (16, 1),
        |id, s, _| Box::new(LogicGateWidget::new(id, LogicOp::Xor, s)),
        |id, _| Box::new(LogicGateProcessNode::new(id, LogicOp::Xor)),
    ));
    table.push(node(
        "Logic", "NOT", (1, 1),
        |id, s, _| Box::new(LogicGateWidget::new(id, LogicOp::Not, s)),
        |id, _| Box::new(LogicGateProcessNode::new(id, LogicOp::Not)),
    ));
    table.push(node(
        "Logic", "SR Flip-Flop", (2, 2),
        |id, s, _| Box::new(FlipFlopWidget::new(id, s, FlipFlopKind::Sr)),
        |id, _| Box::new(FlipFlopProcessNode::new(id)),
    ));
    table.push(node(
        "Logic", "JK Flip-Flop", (3, 2),
        |id, s, _| Box::new(FlipFlopWidget::new(id, s, FlipFlopKind::Jk)),
        |id, _| Box::new(JkFlipFlopProcessNode::new(id)),
    ));
    table.push(node(
        "Position", "Pan Tilt to Position", (2, 2),
        |id, s, _| Box::new(PositionMergeWidget::new(id, s)),
        |id, _| Box::new(PositionMergeProcessNode::new(id)),
    ));
    table.push(node(
        "Position", "Position to Pan Tilt", (2, 2),
        |id, s, _| Box::new(PositionSplitWidget::new(id, s)),
        |id, _| Box::new(PositionSplitProcessNode::new(id)),
    ));
    table.push(node(
        "Display", "Scope", (8, 0),
        |id, s, _| Box::new(ScopeWidget::new(id, s)),
        |id, _| Box::new(ScopeProcessNode::new(id)),
    ));
    // Sized for the largest mode (Gradient = 40 channels).
    table.push(node(
        "Display", "Color Display", (40, 0),
        |id, s, _| Box::new(ColorDisplayWidget::new(id, s)),
        |id, _| Box::new(ColorDisplayProcessNode::new(id)),
    ));
    table.push(node(
        "Display", "Value Display", (1, 0),
        |id, s, _| Box::new(ValueDisplayWidget::new(id, s)),
        |id, _| Box::new(ValueDisplayProcessNode::new(id)),
    ));
    for kind in MeterKind::ALL {
        table.push(node(
            "Display", kind.type_name(), (1, 0),
            move |id, s, _| Box::new(MeterWidget::new(id, s, kind)),
            move |id, _| Box::new(MeterProcessNode::new(id, kind)),
        ));
    }
    table.push(node(
        "Display", "LED Display", (1, 0),
        |id, s, _| Box::new(LedDisplayWidget::new(id, s)),
        |id, _| Box::new(LedDisplayProcessNode::new(id)),
    ));
    // Inputs: in, min, max (3). Output: out (1).
    table.push(node(
        "Math", "Range to Unit", (3, 1),
        |id, s, _| Box::new(ScalerWidget::new(id, s)),
        |id, _| Box::new(ScalerProcessNode::new(id)),
    ));
    // Inputs: in, scale, offset (3). Output: out (1).
    table.push(node(
        "Math", "Scale Offset", (3, 1),
        |id, s, _| Box::new(ScaleOffsetWidget::new(id, s)),
        |id, _| Box::new(ScaleOffsetProcessNode::new(id)),
    ));
    // Inputs: in, range, center (3). Output: out (1).
    table.push(node(
        "Math", "Bipolar", (3, 1),
        |id, s, _| Box::new(BipolarWidget::new(id, s)),
        |id, _| Box::new(BipolarProcessNode::new(id)),
    ));
    // 1 select + MUX_MAX_SLOTS * 40 inputs, 40 outputs.
    table.push(node(
        "Route", "Multiplexer", (1 + engine::nodes::math::multiplex::MUX_MAX_SLOTS * 40, 40),
        |id, s, _| Box::new(MultiplexerWidget::new(id, s)),
        |id, _| Box::new(MultiplexerProcessNode::new(id)),
    ));
    // 1 select + 40 in, MUX_MAX_SLOTS * 40 outputs.
    table.push(node(
        "Route", "Demultiplexer", (1 + 40, engine::nodes::math::multiplex::MUX_MAX_SLOTS * 40),
        |id, s, _| Box::new(DemultiplexerWidget::new(id, s)),
        |id, _| Box::new(DemultiplexerProcessNode::new(id)),
    ));
    // 2 inputs, Palette output = 12 channels
    table.push(node(
        "Color", "Palette Select", (2, 12),
        |id, s, ctx| Box::new(PaletteSelectWidget::new(id, s, ctx.palette_ctx.clone())),
        |id, _| Box::new(PaletteSelectProcessNode::new(id)),
    ));
    // No inputs, Gradient output = 40 channels (8 stops × 5 floats).
    table.push(node(
        "Color", "Gradient Source", (0, 40),
        |id, s, ctx| Box::new(GradientSourceWidget::new(id, s, ctx.gradient_library.clone())),
        |id, _| Box::new(GradientSourceProcessNode::new(id)),
    ));
    // Sized for the largest mode (Gradient = 40 ch main + 1 amount), same out.
    table.push(node(
        "Color", "Color Modifier", (40 + 1, 40),
        |id, s, _| Box::new(ColorModifierWidget::new(id, s)),
        |id, _| Box::new(ColorModifierProcessNode::new(id)),
    ));
    // Inputs: 12-ch palette + 4 position channels = 16. Output: 40-ch Gradient.
    table.push(node(
        "Color", "Palette to Gradient", (16, 40),
        |id, s, _| Box::new(PaletteToGradientWidget::new(id, s)),
        |id, _| Box::new(PaletteToGradientProcessNode::new(id)),
    ));
    // Start with generous shared state; will resize as ports are added.
    table.push(node(
        "Route", "Subgraph", (12, 12),
        |id, s, _| Box::new(SubgraphWidget::new(id, s)),
        |id, _| Box::new(SubgraphProcessNode::new(id)),
    ));
    table.push(node(
        "Portal", "Portal Send", (64, 0),
        |id, s, _| Box::new(widgets::nodes::meta::portal::PortalSendWidget::new(id, s)),
        |id, ctx| Box::new(engine::nodes::meta::portal::PortalSendProcessNode::new(id, ctx.portals.clone())),
    ));
    table.push(node(
        "Portal", "Portal Receive", (0, 64),
        |id, s, ctx| Box::new(widgets::nodes::meta::portal::PortalReceiveWidget::new(id, s, ctx.portals.clone())),
        |id, ctx| Box::new(engine::nodes::meta::portal::PortalReceiveProcessNode::new(id, ctx.portals.clone())),
    ));
    table.push(node(
        "Effect", "Effect Stack", (3, 0),
        |id, s, ctx| Box::new(EffectStackWidget::new(id, s, ctx.group_ctx.clone(), ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectStackProcessNode::new(id, ctx.effect_hub.clone(), ctx.object_store.clone())),
    ));
    table.push(node(
        "Effect", widgets::nodes::output::effect::FILL.type_name, (48, 0),
        // Room for a trigger or phase plus the 40 gradient channels.
        |id, s, ctx| Box::new(widgets::nodes::output::effect::EffectWidget::new(id, s, widgets::nodes::output::effect::FILL, ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectNode::new(id, Fill::default(), ctx.effect_hub.clone())),
    ));
    table.push(node(
        "Effect", widgets::nodes::output::effect::BAR_LOOP.type_name, (48, 0),
        |id, s, ctx| Box::new(widgets::nodes::output::effect::EffectWidget::new(id, s, widgets::nodes::output::effect::BAR_LOOP, ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectNode::new(id, Bar::default(), ctx.effect_hub.clone())),
    ));
    table.push(node(
        "Effect", widgets::nodes::output::effect::WAVE_LOOP.type_name, (48, 0),
        |id, s, ctx| Box::new(widgets::nodes::output::effect::EffectWidget::new(id, s, widgets::nodes::output::effect::WAVE_LOOP, ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectNode::new(id, Wave::default(), ctx.effect_hub.clone())),
    ));
    table.push(node(
        "Effect", widgets::nodes::output::effect::RIPPLE_BURST.type_name, (48, 0),
        |id, s, ctx| Box::new(widgets::nodes::output::effect::EffectWidget::new(id, s, widgets::nodes::output::effect::RIPPLE_BURST, ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectNode::new(id, Ripple::default(), ctx.effect_hub.clone())),
    ));
    table.push(node(
        "Effect", widgets::nodes::output::effect::SPARK_BURST.type_name, (48, 0),
        |id, s, ctx| Box::new(widgets::nodes::output::effect::EffectWidget::new(id, s, widgets::nodes::output::effect::SPARK_BURST, ctx.stack_names.clone())),
        |id, ctx| Box::new(EffectNode::new(id, SparkBurst::default(), ctx.effect_hub.clone())),
    ));
    // The panel starts a heading whenever the category changes, so the entries
    // of one group must be next to each other.
    table.sort_by_key(|d| CATEGORIES.iter().position(|c| *c == d.category).unwrap_or(CATEGORIES.len()));
    table
}

impl LightBeatApp {
    fn node_context(&self) -> NodeContext {
        NodeContext {
            snapshot: Arc::clone(&self.snapshot),
            input_controllers: self.input_controllers.shared.clone(),
            audio_inputs: self.audio_inputs.shared.clone(),
            dmx_inputs: self.dmx_inputs.clone(),
            midi_hub: self.midi_hub.clone(),
            midi_learned: self.midi_learned.clone(),
            gradient_library: self.gradient_library.clone(),
            palette_ctx: self.palette_ctx.clone(),
            group_ctx: self.group_ctx.clone(),
            stack_names: self.stack_names.clone(),
            portals: self.portals.clone(),
            effect_hub: self.effect_hub.clone(),
            object_store: self.object_store.clone(),
            wanted_beats: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(crate) fn register_node_factories(&mut self) {
        // The graph is rebuilt on load and undo; the effects' stack picker
        // reads names through this link, so it has to be remade each time.
        self.graph.set_stack_names(self.stack_names.clone());
        let ctx = self.node_context();
        for desc in node_table() {
            let (inputs, outputs) = desc.channels;
            let ctx = ctx.clone();
            self.graph.register_node(desc.category, desc.name, move |id| {
                (desc.widget)(id, new_shared_state(inputs, outputs), &ctx)
            });
        }
    }

    pub(crate) fn wire_new_nodes(&mut self) {
        // Handle nodes created by context menu / paste / duplicate.
        let new_nodes = self.graph.drain_new_nodes();
        if new_nodes.is_empty() {
            return;
        }
        let ctx = self.node_context();
        let table = node_table();
        for new_node in new_nodes {
            let node = self.graph.node_mut_at_path(new_node.index, &new_node.subgraph_path);
            let type_name = node.type_name();
            let id = node.node_id();
            let shared = Arc::clone(node.shared_state());
            let subgraph_path = new_node.subgraph_path;

            // Create corresponding engine node.
            let engine_node: Option<Box<dyn ProcessNode>> =
                match table.iter().find(|d| d.name == type_name) {
                    Some(desc) => Some((desc.engine)(id, &ctx)),
                    None => {
                        eprintln!("Unknown node type for engine: {}", type_name);
                        None
                    }
                };
            for (pattern, listener) in ctx.take_wanted_beats() {
                self._subs.push(self.beat_clock.subscribe(pattern, listener));
            }


            if let Some(engine_node) = engine_node {
                // A node that arrives with promoted params (paste, move into
                // or out of a subgraph) has a fresh engine half that has not
                // been told about them yet.
                {
                    let mut s = shared.lock();
                    if !s.promoted.is_empty() {
                        s.pending_promoted = Some(s.promoted.clone());
                    }
                }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::total_channels;

    fn context() -> NodeContext {
        NodeContext {
            snapshot: Arc::new(parking_lot::Mutex::new(crate::beat_clock::LinkSnapshot::default())),
            input_controllers: crate::input_controller::InputControllerManager::new().shared,
            audio_inputs: crate::audio::manager::AudioInputManager::new().shared,
            dmx_inputs: crate::dmx_input::DmxInputHub::idle(),
            midi_hub: crate::midi_io::MidiHub::idle(),
            midi_learned: widgets::nodes::io::midi_common::new_shared_learned(),
            gradient_library: widgets::nodes::math::gradient_source::new_shared_gradient_library(),
            palette_ctx: widgets::nodes::math::palette_select::new_shared_palette_context(),
            group_ctx: widgets::group_context::new_shared_group_context(),
            stack_names: widgets::nodes::output::effect::new_shared_stack_names(),
            portals: Arc::new(parking_lot::Mutex::new(engine::nodes::meta::portal::PortalRegistry::default())),
            effect_hub: engine::effects::EffectHub::new(),
            object_store: crate::dmx_io::new_shared_object_store(),
            wanted_beats: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[test]
    fn every_entry_makes_both_halves_under_its_own_name() {
        let ctx = context();
        let table = node_table();
        assert!(table.len() > 80);
        let mut names = std::collections::HashSet::new();
        for desc in &table {
            assert!(names.insert(desc.name), "two entries are called {}", desc.name);
            let widget = (desc.widget)(NodeId(1), new_shared_state(desc.channels.0, desc.channels.1), &ctx);
            let engine = (desc.engine)(NodeId(1), &ctx);
            assert_eq!(widget.type_name(), desc.name, "the widget of {} reports another name", desc.name);
            assert_eq!(engine.node_id(), NodeId(1), "{}", desc.name);
        }
    }

    #[test]
    fn a_type_setting_reshapes_the_node_so_it_cannot_be_an_input() {
        let ctx = context();
        for desc in node_table() {
            let engine = (desc.engine)(NodeId(1), &ctx);
            for p in engine.params() {
                if p.name() == "Type" {
                    assert!(engine.non_promotable_params().contains(&"Type"), "{}", desc.name);
                }
            }
        }
    }

    #[test]
    fn every_node_is_in_a_known_category_that_is_also_its_quick_add_group() {
        for desc in node_table() {
            assert!(CATEGORIES.contains(&desc.category), "{} is in {}", desc.name, desc.category);
            let path = widgets::nodes::paths::node_path(desc.category, desc.name);
            assert_eq!(path.split('/').next(), Some(desc.category.to_lowercase().as_str()), "{}", desc.name);
        }
    }

    /// Loads a one-node project saved under `old` into a graph holding every
    /// registered widget and returns the type name of what came out.
    fn loaded_as(old: &str) -> Option<String> {
        let ctx = context();
        let mut g = widgets::nodes::NodeGraph::new();
        for desc in node_table() {
            let (inputs, outputs) = desc.channels;
            let ctx = ctx.clone();
            g.register_node(desc.category, desc.name, move |id| (desc.widget)(id, new_shared_state(inputs, outputs), &ctx));
        }
        let project: crate::project::ProjectFile = serde_json::from_str(&format!(
            r#"{{"nodes": [{{"type_name": "{old}", "id": 1, "pos": [0.0, 0.0], "params": []}}], "connections": []}}"#
        ))
        .unwrap();
        let loaded = crate::project::load_graph(&mut g, &project);
        loaded.first().map(|i| g.node_mut(*i).type_name().to_string())
    }

    #[test]
    fn position_merge_loads_as_pan_tilt_to_position() {
        assert_eq!(loaded_as("Position Merge").as_deref(), Some("Pan Tilt to Position"));
    }

    #[test]
    fn position_split_loads_as_position_to_pan_tilt() {
        assert_eq!(loaded_as("Position Split").as_deref(), Some("Position to Pan Tilt"));
    }

    #[test]
    fn latch_loads_as_tick_delay() {
        assert_eq!(loaded_as("Latch").as_deref(), Some("Tick Delay"));
    }

    #[test]
    fn flip_flop_loads_as_sr_flip_flop() {
        assert_eq!(loaded_as("Flip-Flop").as_deref(), Some("SR Flip-Flop"));
    }

    #[test]
    fn clock_loads_as_link_clock() {
        assert_eq!(loaded_as("Clock").as_deref(), Some("Link Clock"));
    }

    #[test]
    fn scaler_loads_as_range_to_unit() {
        assert_eq!(loaded_as("Scaler").as_deref(), Some("Range to Unit"));
    }

    #[test]
    fn bar_loop_loads_as_bar() {
        assert_eq!(loaded_as("Bar Loop").as_deref(), Some("Bar"));
    }

    #[test]
    fn adsr_loads_as_envelope() {
        assert_eq!(loaded_as("ADSR").as_deref(), Some("Envelope"));
    }

    #[test]
    fn every_rename_points_at_a_registered_node_and_no_old_name_is_registered() {
        let table = node_table();
        for (old, new) in crate::project::RENAMED_NODES {
            assert!(table.iter().any(|d| d.name == new), "{new} is not registered");
            assert!(table.iter().all(|d| d.name != old), "{old} is still registered");
        }
    }

    /// The two halves state their ports separately for the nodes that have not
    /// been moved onto one shared function; a wire is saved by index, so a
    /// difference in order or type breaks projects and one in name misleads.
    #[test]
    fn the_widget_and_the_engine_node_have_the_same_ports() {
        let ctx = context();
        let describe = |ports: Vec<(String, PortType)>| ports.iter().map(|(n, t)| format!("{n}:{t:?}")).collect::<Vec<_>>().join(", ");
        let mut bad = Vec::new();
        for desc in node_table() {
            let widget = (desc.widget)(NodeId(1), new_shared_state(desc.channels.0, desc.channels.1), &ctx);
            let engine = (desc.engine)(NodeId(1), &ctx);
            let w_in: Vec<_> = widget.ui_inputs().iter().filter(|p| p.marker.is_none()).map(|p| (p.def.name.clone(), p.def.port_type)).collect();
            let w_out: Vec<_> = widget.ui_outputs().iter().filter(|p| p.marker.is_none()).map(|p| (p.def.name.clone(), p.def.port_type)).collect();
            let e_in: Vec<_> = engine.inputs().iter().map(|p| (p.name.clone(), p.port_type)).collect();
            let e_out: Vec<_> = engine.outputs().iter().map(|p| (p.name.clone(), p.port_type)).collect();
            // A Portal Send's ports are its widget's own config, which the engine
            // takes on the first tick.
            if w_in != e_in && desc.name != "Portal Send" {
                bad.push(format!("{} inputs: widget [{}] engine [{}]", desc.name, describe(w_in), describe(e_in)));
            }
            // These two name their outputs after the ratio they were set to.
            let relabelled = matches!(desc.name, "Clock Divider" | "Phase Scaler");
            let types = |v: &[(String, PortType)]| v.iter().map(|(_, t)| *t).collect::<Vec<_>>();
            if w_out != e_out && !(relabelled && types(&w_out) == types(&e_out)) {
                bad.push(format!("{} outputs: widget [{}] engine [{}]", desc.name, describe(w_out), describe(e_out)));
            }
        }
        assert!(bad.is_empty(), "ports differ:\n{}", bad.join("\n"));
    }

    #[test]
    fn the_shared_state_is_large_enough_for_the_fresh_engine_node() {
        let ctx = context();
        for desc in node_table() {
            let engine = (desc.engine)(NodeId(1), &ctx);
            let (need_in, need_out) = (total_channels(engine.inputs()), total_channels(engine.outputs()));
            assert!(desc.channels.0 >= need_in, "{}: {} input channels for {}", desc.name, desc.channels.0, need_in);
            assert!(desc.channels.1 >= need_out, "{}: {} output channels for {}", desc.name, desc.channels.1, need_out);
        }
    }
}
