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

/// The group is the category, lowercased: the menus and quick add share one
/// taxonomy.
fn group_of(category: &str) -> String {
    slug(category)
}

/// Nodes with a shorter or older name than their own, as a second way in.
/// Anything not listed is `<category>/<name>`.
fn explicit(label: &str) -> Option<&'static str> {
    Some(match label {
        "MIDI Input" => "input/midi",
        "Input Controller" => "input/controller",
        "Audio Input" => "input/audio",
        "DMX Input" => "input/dmx",
        "Push 1" => "input/push1",
        "Launchpad S" => "input/launchpad",
        "MIDI Output" => "output/midi",
        "Link Clock" => "clock/link",
        "Internal Clock" => "clock/internal",
        "Clock Gen" => "clock/gen",
        "Clock Divider" => "clock/divider",
        "Trigger Delay" => "trigger/delay",
        "Trigger Hold" => "trigger/hold",
        "Tick Delay" => "trigger/latch",
        "Schmitt Trigger" => "trigger/schmitt",
        "Range to Unit" => "math/scaler",
        "Scale Offset" => "math/scale",
        // The symbols are not names.
        ">=" => "compare/gte",
        "<=" => "compare/lte",
        "==" => "compare/eq",
        "!=" => "compare/neq",
        "Pan Tilt to Position" => "position/merge",
        "Position to Pan Tilt" => "position/split",
        "Color Modifier" => "color/modifier",
        "Components to Color" => "color/from-components",
        "Color to Components" => "color/to-components",
        "Value Display" => "display/value",
        "LED Display" => "display/led",
        "Color Display" => "display/color",
        "Peak Level Meter" => "display/peak-meter",
        "Portal Send" => "portal/send",
        "Portal Receive" => "portal/receive",
        "Effect Stack" => "effect/stack",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listed_nodes_get_their_own_path() {
        assert_eq!(node_path("Input", "MIDI Input"), "input/midi");
        assert_eq!(node_path("Input", "Button"), "input/button");
        assert_eq!(node_path("Clock", "Clock Divider"), "clock/divider");
        assert_eq!(node_path("Compare", ">="), "compare/gte");
        assert_eq!(node_path("Signal", "Const Value"), "signal/const-value");
        assert_eq!(node_path("Color", "Color to Components"), "color/to-components");
    }

    #[test]
    fn other_nodes_go_under_their_category_by_name() {
        assert_eq!(node_path("Math", "Mul"), "math/mul");
        assert_eq!(node_path("Math", "Div"), "math/div");
        assert_eq!(node_path("Logic", "JK Flip-Flop"), "logic/jk-flip-flop");
        assert_eq!(node_path("Effect", "Spark Burst"), "effect/spark-burst");
        assert_eq!(node_path("Color", "Palette to Gradient"), "color/palette-to-gradient");
        assert_eq!(node_path("Display", "Level Meter"), "display/level-meter");
        assert_eq!(node_path("Display", "Gauge"), "display/gauge");
        // A node nobody has listed yet still gets a sensible path.
        assert_eq!(node_path("Signal", "Brand New Thing"), "signal/brand-new-thing");
        assert_eq!(node_path("Input", "Some Device"), "input/some-device");
    }

    #[test]
    fn a_slug_is_lowercase_words_with_dashes() {
        assert_eq!(slug("Sample & Hold"), "sample-hold");
        assert_eq!(slug("  Wave  Loop "), "wave-loop");
        assert_eq!(slug("Flip-Flop"), "flip-flop");
    }
}
