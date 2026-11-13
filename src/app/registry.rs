use crate::LightBeatApp;
use crate::engine;
use crate::widgets;
use std::sync::Arc;
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

impl LightBeatApp {
    pub(crate) fn register_node_factories(&mut self) {
        // The graph is rebuilt on load and undo; the effects' stack picker
        // reads names through this link, so it has to be remade each time.
        self.graph.set_stack_names(self.stack_names.clone());
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
        let dmx_hub = self.dmx_inputs.clone();
        self.graph.register_node("IO", "DMX Input", move |id| {
            // The status port and up to 64 entries of at most three channels.
            Box::new(DmxInputWidget::new(id, new_shared_state(0, 200), dmx_hub.clone()))
        });
        let (hub, learned) = (self.midi_hub.clone(), self.midi_learned.clone());
        self.graph.register_node("IO", "MIDI Input", move |id| {
            // The status port and up to 32 controls.
            Box::new(MidiInputWidget::new(id, new_shared_state(0, 33), hub.clone(), learned.clone()))
        });
        let (hub, learned) = (self.midi_hub.clone(), self.midi_learned.clone());
        self.graph.register_node("IO", "MIDI Output", move |id| {
            Box::new(MidiOutputWidget::new(id, new_shared_state(32, 1), hub.clone(), learned.clone()))
        });
        let ic_shared_push = self.input_controllers.shared.clone();
        self.graph.register_node("IO", "Push 1", move |id| {
            // 26 outputs (incl. "connected"), 21 inputs (see push1.rs). 32/32 gives headroom.
            Box::new(Push1Widget::new(id, new_shared_state(32, 32), ic_shared_push.clone()))
        });
        let ic_shared_x1 = self.input_controllers.shared.clone();
        self.graph.register_node("IO", "X1", move |id| {
            // 34 button outputs + 4 encoder outputs + 8 pot outputs + "any change" +
            // "connected" = 48 outputs. 34 LED feedback inputs. 56/56 leaves headroom.
            Box::new(X1Widget::new(id, new_shared_state(56, 56), ic_shared_x1.clone()))
        });
        let ic_shared_lp = self.input_controllers.shared.clone();
        self.graph.register_node("IO", "Launchpad S", move |id| {
            // 82 outputs = 64 pads + 8 side + 8 top + "any change" + "connected".
            // 80 LED feedback inputs, each `Color` (3 channels) = 240
            // channels. 256 gives a bit of headroom.
            Box::new(LaunchpadWidget::new(id, new_shared_state(256, 82), ic_shared_lp.clone()))
        });
        let ai_shared = self.audio_inputs.shared.clone();
        self.graph.register_node("IO", "Audio Input", move |id| {
            Box::new(AudioInputWidget::new(id, new_shared_state(0, 32), ai_shared.clone()))
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
        self.graph.register_node("UI", "XY Pad", |id| {
            // No inputs; outputs x and y (0..1).
            Box::new(XyPadWidget::new(id, new_shared_state(0, 2)))
        });
        self.graph.register_node("UI", "Color Mixer", |id| {
            Box::new(ColorMixerWidget::new(id, new_shared_state(0, 3)))
        });
        self.graph.register_node("UI", "Gradient Stops", |id| {
            // 1 Palette input (12 channels) for the live preview, 4 position
            // outputs to feed Palette → Gradient's pos1..pos4.
            Box::new(GradientStopsWidget::new(id, new_shared_state(12, 4)))
        });
        self.graph.register_node("UI", "Peak Level Meter", |id| {
            Box::new(PeakMeterWidget::new(id, new_shared_state(2, 0)))
        });

        // Transport
        self.graph.register_node("Transport", "Phase Scaler", |id| {
            Box::new(PhaseScalerWidget::new(id, new_shared_state(4, 16)))
        });
        self.graph.register_node("Transport", "LFO", |id| {
            Box::new(LfoWidget::new(id, new_shared_state(1, 2)))
        });
        self.graph.register_node("Transport", "Step Sequencer", |id| {
            Box::new(StepSequencerWidget::new(id, new_shared_state(2, 3)))
        });
        self.graph.register_node("Transport", "Envelope", |id| {
            Box::new(EnvelopeWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Transport", "Trigger Delay", |id| {
            Box::new(TriggerDelayWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Trigger Hold", |id| {
            Box::new(TriggerHoldWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Formula", |id| {
            Box::new(FormulaWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Math", "Random", |id| {
            Box::new(RandomWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Sample & Hold", |id| {
            Box::new(SampleHoldWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Transport", "Smoothing", |id| {
            Box::new(SmoothingWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Latch", |id| {
            Box::new(LatchWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Clock Divider", |id| {
            Box::new(ClockDividerWidget::new(id, new_shared_state(1, 16)))
        });
        self.graph.register_node("Transport", "Clock Gen", |id| {
            Box::new(ClockGenWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Time to Phase", |id| {
            Box::new(TimeToPhaseWidget::new(id, new_shared_state(1, 2)))
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
        self.graph.register_node("Color", "Components to Color", |id| {
            Box::new(ComponentsToColorWidget::new(id, new_shared_state(4, 4)))
        });
        self.graph.register_node("Color", "Color to Components", |id| {
            Box::new(ColorToComponentsWidget::new(id, new_shared_state(4, 4)))
        });
        for kind in ReshapeKind::ALL {
            self.graph.register_node("Color", kind.type_name(), move |id| {
                // Room for a full gradient (40 channels) on either side.
                Box::new(ColorReshapeWidget::new(id, new_shared_state(48, 48), kind))
            });
        }
        let gradient_library = self.gradient_library.clone();
        self.graph.register_node("Route", "Lookup Table", move |id| {
            // Outputs: 1 (rows) + worst case 8 columns × Gradient (40 ch) = 321.
            Box::new(LookupWidget::new(id, new_shared_state(1, 321), gradient_library.clone()))
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
            // Inputs: trigger, reset, max (3 channels). Outputs: count, wrap (2).
            Box::new(CounterWidget::new(id, new_shared_state(3, 2)))
        });
        self.graph.register_node("Math", "Change Detect", |id| {
            Box::new(ChangeDetectWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Math", "Schmitt Trigger", |id| {
            Box::new(SchmittTriggerWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Math", "Toggle Bank", |id| {
            // Up to 16 channels, each Logic in and out.
            Box::new(ToggleBankWidget::new(id, new_shared_state(16, 16)))
        });
        self.graph.register_node("Math", "Trigger Bank", |id| {
            // Up to 16 channels, each Untyped in and Logic out.
            Box::new(TriggerBankWidget::new(id, new_shared_state(16, 16)))
        });
        self.graph.register_node("Math", "Gain", |id| {
            Box::new(GainWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Math", "Phase Accumulator", |id| {
            Box::new(PhaseAccumulatorWidget::new(id, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Port Hub", |id| {
            // Max channels = MUX_MAX_SLOTS (16) × max channels per type
            // (Gradient = 40) = 640. Round up to 1024 for headroom.
            Box::new(PortHubWidget::new(id, new_shared_state(1024, 1024)))
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
            Box::new(ScopeWidget::new(id, new_shared_state(8, 0)))
        });
        self.graph.register_node("Color", "Color Display", |id| {
            // Sized for the largest mode (Gradient = 40 channels).
            Box::new(ColorDisplayWidget::new(id, new_shared_state(40, 0)))
        });
        self.graph.register_node("Display", "Value Display", |id| {
            Box::new(ValueDisplayWidget::new(id, new_shared_state(1, 0)))
        });
        for kind in [MeterKind::Level, MeterKind::Gauge] {
            self.graph.register_node("Display", kind.type_name(), move |id| {
                Box::new(MeterWidget::new(id, new_shared_state(1, 0), kind))
            });
        }
        self.graph.register_node("Display", "LED Display", |id| {
            Box::new(LedDisplayWidget::new(id, new_shared_state(1, 0)))
        });

        // Math - Scaler
        self.graph.register_node("Math", "Scaler", |id| {
            // Inputs: in, min, max (3). Output: out (1).
            Box::new(ScalerWidget::new(id, new_shared_state(3, 1)))
        });
        self.graph.register_node("Math", "Scale Offset", |id| {
            // Inputs: in, scale, offset (3). Output: out (1).
            Box::new(ScaleOffsetWidget::new(id, new_shared_state(3, 1)))
        });
        self.graph.register_node("Math", "Bipolar", |id| {
            // Inputs: in, range, center (3). Output: out (1).
            Box::new(BipolarWidget::new(id, new_shared_state(3, 1)))
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
        let gradient_library = self.gradient_library.clone();
        self.graph.register_node("Color", "Gradient Source", move |id| {
            // No inputs, Gradient output = 40 channels (8 stops × 5 floats).
            Box::new(GradientSourceWidget::new(
                id, new_shared_state(0, 40), gradient_library.clone(),
            ))
        });
        self.graph.register_node("Color", "Color Modifier", |id| {
            // Sized for the largest mode (Gradient = 40 ch main + 1 amount), same out.
            Box::new(ColorModifierWidget::new(id, new_shared_state(40 + 1, 40)))
        });
        self.graph.register_node("Color", "Palette to Gradient", |id| {
            // Inputs: 12-ch palette + 4 position channels = 16. Output: 40-ch Gradient.
            Box::new(PaletteToGradientWidget::new(id, new_shared_state(16, 40)))
        });

        // Meta
        self.graph.register_node("Meta", "Subgraph", |id| {
            // Start with generous shared state; will resize as ports are added.
            Box::new(SubgraphWidget::new(id, new_shared_state(12, 12)))
        });
        self.graph.register_node("Meta", "Portal Send", |id| {
            Box::new(widgets::nodes::meta::portal::PortalSendWidget::new(
                id, new_shared_state(64, 0),
            ))
        });
        let portals_for_receive = self.portals.clone();
        self.graph.register_node("Meta", "Portal Receive", move |id| {
            Box::new(widgets::nodes::meta::portal::PortalReceiveWidget::new(
                id, new_shared_state(0, 64), portals_for_receive.clone(),
            ))
        });

        // Output
        let gctx2 = self.group_ctx.clone();
        let stack_names = self.stack_names.clone();
        self.graph.register_node("Effects", "Effect Stack", move |id| {
            Box::new(EffectStackWidget::new(id, new_shared_state(3, 0), gctx2.clone(), stack_names.clone()))
        });
        for spec in [
            widgets::nodes::output::effect::FILL,
            widgets::nodes::output::effect::BAR_LOOP,
            widgets::nodes::output::effect::WAVE_LOOP,
            widgets::nodes::output::effect::RIPPLE_BURST,
            widgets::nodes::output::effect::SPARK_BURST,
        ] {
            let names = self.stack_names.clone();
            self.graph.register_node("Effects", spec.type_name, move |id| {
                // Room for a trigger or phase plus the 40 gradient channels.
                Box::new(widgets::nodes::output::effect::EffectWidget::new(id, new_shared_state(48, 0), spec, names.clone()))
            });
        }
    }

    pub(crate) fn wire_new_nodes(&mut self) {
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
                "DMX Input" => Some(Box::new(DmxInputProcessNode::new(id, self.dmx_inputs.clone()))),
                "MIDI Input" => Some(Box::new(MidiInputProcessNode::new(id, self.midi_hub.clone()))),
                "MIDI Output" => Some(Box::new(MidiOutputProcessNode::new(id, self.midi_hub.clone()))),
                "Push 1" => Some(Box::new(Push1ProcessNode::new(
                    id, self.input_controllers.shared.clone(),
                ))),
                "X1" => Some(Box::new(X1ProcessNode::new(
                    id, self.input_controllers.shared.clone(),
                ))),
                "Launchpad S" => Some(Box::new(LaunchpadProcessNode::new(
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
                "Color Mixer" => Some(Box::new(ColorMixerProcessNode::new(id))),
                "XY Pad" => Some(Box::new(XyPadProcessNode::new(id))),
                "Gradient Stops" => Some(Box::new(GradientStopsProcessNode::new(id))),
                "Phase Scaler" => Some(Box::new(PhaseScalerProcessNode::new(id))),
                "Time to Phase" => Some(Box::new(TimeToPhaseProcessNode::new(id))),
                "LFO" => Some(Box::new(LfoProcessNode::new(id))),
                "Step Sequencer" => Some(Box::new(StepSequencerProcessNode::new(id))),
                "Scope" => Some(Box::new(ScopeProcessNode::new(id))),
                "Envelope" => Some(Box::new(EnvelopeProcessNode::new(id))),
                "Trigger Delay" => Some(Box::new(TriggerDelayProcessNode::new(id))),
                "Trigger Hold" => Some(Box::new(TriggerHoldProcessNode::new(id))),
                "Formula" => Some(Box::new(FormulaProcessNode::new(id))),
                "Random" => Some(Box::new(RandomProcessNode::new(id))),
                "Sample & Hold" => Some(Box::new(SampleHoldProcessNode::new(id))),
                "Smoothing" => Some(Box::new(SmoothingProcessNode::new(id))),
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
                "Level Meter" => Some(Box::new(MeterProcessNode::new(id, MeterKind::Level))),
                "Gauge" => Some(Box::new(MeterProcessNode::new(id, MeterKind::Gauge))),
                "LED Display" => Some(Box::new(LedDisplayProcessNode::new(id))),
                "Palette Select" => Some(Box::new(PaletteSelectProcessNode::new(id))),
                "Gradient Source" => Some(Box::new(GradientSourceProcessNode::new(id))),
                "Color Modifier" => Some(Box::new(ColorModifierProcessNode::new(id))),
                "Palette to Gradient" => Some(Box::new(PaletteToGradientProcessNode::new(id))),
                "Multiplexer" => Some(Box::new(MultiplexerProcessNode::new(id))),
                "Demultiplexer" => Some(Box::new(DemultiplexerProcessNode::new(id))),
                "Scaler" => Some(Box::new(ScalerProcessNode::new(id))),
                "Bipolar" => Some(Box::new(BipolarProcessNode::new(id))),
                "Scale Offset" => Some(Box::new(ScaleOffsetProcessNode::new(id))),
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
                "Components to Color" => Some(Box::new(ComponentsToColorProcessNode::new(id))),
                "Color to Components" => Some(Box::new(ColorToComponentsProcessNode::new(id))),
                name if ReshapeKind::ALL.iter().any(|k| k.type_name() == name) => {
                    let kind = *ReshapeKind::ALL.iter().find(|k| k.type_name() == name).unwrap();
                    Some(Box::new(ColorReshapeProcessNode::new(id, kind)))
                }
                "Position Merge" => Some(Box::new(PositionMergeProcessNode::new(id))),
                "Position Split" => Some(Box::new(PositionSplitProcessNode::new(id))),
                "Lookup Table" => Some(Box::new(LookupProcessNode::new(id))),
                "Counter" => Some(Box::new(CounterProcessNode::new(id))),
                "Change Detect" => Some(Box::new(ChangeDetectProcessNode::new(id))),
                "Schmitt Trigger" => Some(Box::new(SchmittTriggerProcessNode::new(id))),
                "Toggle Bank" => Some(Box::new(ToggleBankProcessNode::new(id))),
                "Trigger Bank" => Some(Box::new(TriggerBankProcessNode::new(id))),
                "Gain" => Some(Box::new(GainProcessNode::new(id))),
                "Phase Accumulator" => Some(Box::new(PhaseAccumulatorProcessNode::new(id))),
                "Port Hub" => Some(Box::new(PortHubProcessNode::new(id))),
                "Const Value" => Some(Box::new(ConstantProcessNode::new(id, PortType::Untyped, 0.0))),
                "Const Logic" => Some(Box::new(ConstantProcessNode::new(id, PortType::Logic, 0.0))),
                "Const Phase" => Some(Box::new(ConstantProcessNode::new(id, PortType::Phase, 0.0))),
                "Sin" => Some(Box::new(OscillatorProcessNode::new(id, OscFunc::Sin))),
                "Cos" => Some(Box::new(OscillatorProcessNode::new(id, OscFunc::Cos))),
                "Subgraph" => Some(Box::new(SubgraphProcessNode::new(id))),
                "Portal Send" => Some(Box::new(
                    engine::nodes::meta::portal::PortalSendProcessNode::new(id, self.portals.clone()),
                )),
                "Portal Receive" => Some(Box::new(
                    engine::nodes::meta::portal::PortalReceiveProcessNode::new(id, self.portals.clone()),
                )),
                "Effect Stack" => Some(Box::new(EffectStackProcessNode::new(id, self.effect_hub.clone(), self.object_store.clone()))),
                "Fill" => Some(Box::new(EffectNode::new(id, Fill::default(), self.effect_hub.clone()))),
                "Bar Loop" => Some(Box::new(EffectNode::new(id, Bar::default(), self.effect_hub.clone()))),
                "Wave Loop" => Some(Box::new(EffectNode::new(id, Wave::default(), self.effect_hub.clone()))),
                "Ripple Burst" => Some(Box::new(EffectNode::new(id, Ripple::default(), self.effect_hub.clone()))),
                "Spark Burst" => Some(Box::new(EffectNode::new(id, SparkBurst::default(), self.effect_hub.clone()))),
                _ => {
                    eprintln!("Unknown node type for engine: {}", type_name);
                    None
                }
            };

            if let Some(engine_node) = engine_node {
                // A node that arrives with promoted params (paste, move into
                // or out of a subgraph) has a fresh engine half that has not
                // been told about them yet.
                {
                    let mut s = shared.lock().unwrap();
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
