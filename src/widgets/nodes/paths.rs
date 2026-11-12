//! Where each node lives in the path-like names used to find it, such as
//! `input/midi` or `math/mul`. The first part is the group.

/// The path of the node `label` registered under `category`.
pub fn node_path(category: &str, label: &str) -> String {
    if let Some(path) = explicit(label) {
        return path.to_string();
    }
    format!("{}/{}", group_of(category), slug(label))
}

/// Lowercase words joined by dashes.
fn slug(label: &str) -> String {
    let mut out = String::new();
    for c in label.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The group a node with no path of its own goes in, from its category.
fn group_of(category: &str) -> String {
    match category {
        "IO" | "UI" => "input",
        "Transport" => "signal",
        "Effects" => "effect",
        "Meta" => "route",
        other => return slug(other),
    }
    .to_string()
}

/// Nodes whose name alone would put them in the wrong place or say too little.
fn explicit(label: &str) -> Option<&'static str> {
    Some(match label {
        // Where signals come from.
        "MIDI Input" => "input/midi",
        "Input Controller" => "input/controller",
        "Audio Input" => "input/audio",
        "DMX Input" => "input/dmx",
        "Push 1" => "input/push1",
        "X1" => "input/x1",
        "Launchpad S" => "input/launchpad",
        "Button" => "input/button",
        "Fader" => "input/fader",
        "Button Group" => "input/button-group",
        "Fader Group" => "input/fader-group",
        "XY Pad" => "input/xy-pad",
        "Color Mixer" => "input/color-mixer",
        "Gradient Stops" => "input/gradient-stops",
        // Where they go.
        "MIDI Output" => "output/midi",
        // Time and phase.
        "Clock" => "clock/link",
        "Internal Clock" => "clock/internal",
        "Clock Gen" => "clock/gen",
        "Clock Divider" => "clock/divider",
        "Phase Scaler" => "clock/phase-scaler",
        "Time to Phase" => "clock/time-to-phase",
        "Phase Accumulator" => "clock/phase-accumulator",
        // Making and shaping signals.
        "LFO" => "signal/lfo",
        "Envelope" => "signal/envelope",
        "Step Sequencer" => "signal/step-sequencer",
        "Smoothing" => "signal/smoothing",
        "Transition" => "signal/transition",
        "Sample & Hold" => "signal/sample-hold",
        "Random" => "signal/random",
        "Const Value" => "signal/const-value",
        "Const Logic" => "signal/const-logic",
        "Const Phase" => "signal/const-phase",
        // Triggers and events.
        "Trigger Delay" => "trigger/delay",
        "Trigger Hold" => "trigger/hold",
        "Latch" => "trigger/latch",
        "Counter" => "trigger/counter",
        "Change Detect" => "trigger/change-detect",
        "Schmitt Trigger" => "trigger/schmitt",
        "Toggle Bank" => "trigger/toggle-bank",
        "Trigger Bank" => "trigger/trigger-bank",
        // Arithmetic.
        "Formula" => "math/formula",
        "Gain" => "math/gain",
        "Scaler" => "math/scaler",
        "Bipolar" => "math/bipolar",
        "Scale Offset" => "math/scale",
        "Lookup Table" => "route/lookup-table",
        // Comparing: the symbols are not names.
        ">=" => "compare/gte",
        "<=" => "compare/lte",
        "==" => "compare/eq",
        "!=" => "compare/neq",
        // Positions.
        "Position Merge" => "position/merge",
        "Position Split" => "position/split",
        // Colour.
        "Color Modifier" => "color/modifier",
        "Components to Color" => "color/from-components",
        "Color to Components" => "color/to-components",
        // Looking at values.
        "Value Display" => "display/value",
        "LED Display" => "display/led",
        "Color Display" => "display/color",
        "Peak Level Meter" => "display/peak-meter",
        // Getting signals from one place to another.
        "Port Hub" => "route/port-hub",
        "Multiplexer" => "route/multiplexer",
        "Demultiplexer" => "route/demultiplexer",
        "Subgraph" => "route/subgraph",
        "Output Portal TX" => "portal/output-tx",
        "Output Portal RX" => "portal/output-rx",
        "Input Portal RX" => "portal/input-rx",
        "Input Portal TX" => "portal/input-tx",
        // Lights.
        "Effect Stack" => "effect/stack",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_nodes_get_their_own_path() {
        assert_eq!(node_path("IO", "MIDI Input"), "input/midi");
        assert_eq!(node_path("UI", "Button"), "input/button");
        assert_eq!(node_path("Transport", "Clock Divider"), "clock/divider");
        assert_eq!(node_path("Compare", ">="), "compare/gte");
        assert_eq!(node_path("Math", "Const Value"), "signal/const-value");
        assert_eq!(node_path("Color", "Color to Components"), "color/to-components");
    }

    #[test]
    fn other_nodes_go_under_their_category_by_name() {
        assert_eq!(node_path("Math", "Mul"), "math/mul");
        assert_eq!(node_path("Math", "Div"), "math/div");
        assert_eq!(node_path("Logic", "JK Flip-Flop"), "logic/jk-flip-flop");
        assert_eq!(node_path("Effects", "Spark Burst"), "effect/spark-burst");
        assert_eq!(node_path("Color", "Palette to Gradient"), "color/palette-to-gradient");
        assert_eq!(node_path("Display", "Level Meter"), "display/level-meter");
        assert_eq!(node_path("Display", "Gauge"), "display/gauge");
        // A node nobody has listed yet still gets a sensible path.
        assert_eq!(node_path("Transport", "Brand New Thing"), "signal/brand-new-thing");
        assert_eq!(node_path("IO", "Some Device"), "input/some-device");
    }

    #[test]
    fn a_slug_is_lowercase_words_with_dashes() {
        assert_eq!(slug("Sample & Hold"), "sample-hold");
        assert_eq!(slug("  Wave  Loop "), "wave-loop");
        assert_eq!(slug("Flip-Flop"), "flip-flop");
    }
}
