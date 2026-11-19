//! The format history of a project level: the project file, every subgraph's
//! inner graph and the graph of a macro.
//!
//! Wires are saved by port index, params by index, promoted inputs and
//! snapshot values by setting name and nodes by type name, so a change to any
//! of those in an existing node type needs a step here.
//!
//! 1: hardware nodes gained a leading "connected" output.
//! 2: Change Detect has its value first.
//! 3: Envelope and Transition have their signal first and the trigger after.
//! 4: the ports some nodes built themselves for "a wire overrides this
//!    setting" became promoted params.
//! 5: node types and settings carry their current names; Sin and Cos are LFOs.
//! 6: the Launchpad S, the groups and the banks gained a bus output.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::{Schema, Step};

pub const SCHEMA: Schema = Schema {
    name: "project",
    baseline: 0,
    steps: &[
        Step { to: 1, apply: shift_outputs_past_the_status_port },
        Step { to: 2, apply: change_detect_value_first },
        Step { to: 3, apply: signal_before_trigger },
        Step { to: 4, apply: override_ports_to_promoted },
        Step { to: 5, apply: current_names },
        Step { to: 6, apply: bus_outputs_first },
    ],
    nested: inner_graphs,
};

fn inner_graphs(level: &mut Value) -> Vec<&mut Value> {
    nodes_mut(level).filter_map(|n| n.get_mut("inner_graph")).filter(|g| g.is_object()).collect()
}

fn nodes_mut(level: &mut Value) -> impl Iterator<Item = &mut Value> {
    level.get_mut("nodes").and_then(Value::as_array_mut).into_iter().flatten()
}

fn connections_mut(level: &mut Value) -> impl Iterator<Item = &mut Value> {
    level.get_mut("connections").and_then(Value::as_array_mut).into_iter().flatten()
}

/// Node id to the type name it has since version 5. Renames were not tied to
/// a version before that, so the steps up to 4 can meet either name.
fn node_types(level: &Value) -> HashMap<u64, &'static str> {
    let nodes = level.get("nodes").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    nodes
        .iter()
        .filter_map(|n| Some((n.get("id")?.as_u64()?, name_at_5(n.get("type_name")?.as_str()?)?)))
        .collect()
}

/// Rewrite one end of every wire (`to_port` or `from_port` of the node at
/// `to_node` or `from_node`) for the node types `remap` knows.
fn remap_ports(level: &mut Value, node_key: &str, port_key: &str, remap: impl Fn(u64, &str, u64) -> u64) {
    let types = node_types(level);
    for c in connections_mut(level) {
        let (Some(node), Some(port)) = (c.get(node_key).and_then(Value::as_u64), c.get(port_key).and_then(Value::as_u64))
        else {
            continue;
        };
        if let Some(type_name) = types.get(&node) {
            c[port_key] = Value::from(remap(node, type_name, port));
        }
    }
}

// --- 1 ----------------------------------------------------------------------

fn shift_outputs_past_the_status_port(level: &mut Value) {
    const HARDWARE: [&str; 5] = ["Input Controller", "Audio Input", "Launchpad S", "Push 1", "X1"];
    remap_ports(level, "from_node", "from_port", |_, t, port| if HARDWARE.contains(&t) { port + 1 } else { port });
}

// --- 2 ----------------------------------------------------------------------

/// Was (trigger, in).
fn change_detect_value_first(level: &mut Value) {
    remap_ports(level, "to_node", "to_port", |_, t, port| if t == "Change Detect" && port < 2 { 1 - port } else { port });
}

// --- 3 ----------------------------------------------------------------------

fn signal_before_trigger(level: &mut Value) {
    // Transition in "Phase out" mode (param 0 = 4) has no `in` and kept its order.
    let phase_out: Vec<u64> = nodes_mut(level)
        .filter(|n| {
            let params = n.get("params").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
            params.iter().any(|p| p.get("index").and_then(Value::as_u64) == Some(0) && number(p.get("value")) == Some(4.0))
        })
        .filter_map(|n| n.get("id")?.as_u64())
        .collect();
    remap_ports(level, "to_node", "to_port", |node, t, port| match (t, port) {
        // Was (gate, in).
        ("Envelope", 0 | 1) => 1 - port,
        // Was (trigger, phase, in).
        ("Transition", 0 | 1) if !phase_out.contains(&node) => port + 1,
        ("Transition", 2) if !phase_out.contains(&node) => 0,
        _ => port,
    });
}

/// A saved param value is a bare number or boolean.
fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        other => other.as_f64(),
    }
}

// --- 4 ----------------------------------------------------------------------

/// These nodes had input ports of their own for some settings. The promoted
/// ports of the listed settings sit where those ports were, in the same order,
/// so the node gets that list and its wires stay on the same indices. What the
/// node had promoted already comes after them, as its ports did.
fn override_ports_to_promoted(level: &mut Value) {
    const OVERRIDES: [(&str, &[(&str, &str)]); 7] = [
        ("Range to Unit", &[("In Min", "Float"), ("In Max", "Float")]),
        ("Scale Offset", &[("Scale", "Float"), ("Offset", "Float")]),
        ("Bipolar", &[("Range", "Float"), ("Center", "Float")]),
        ("Counter", &[("Max", "Int")]),
        ("Phase Scaler", &[("Exponent", "Int"), ("Offset", "Float")]),
        ("Palette to Gradient", &[("Pos 1", "Float"), ("Pos 2", "Float"), ("Pos 3", "Float"), ("Pos 4", "Float")]),
        ("Trigger Hold", &[("Duration", "Float")]),
    ];
    for node in nodes_mut(level) {
        let type_name = node.get("type_name").and_then(Value::as_str).and_then(name_at_5);
        let Some((_, settings)) = OVERRIDES.iter().find(|(t, _)| Some(*t) == type_name) else { continue };
        let mut promoted: Vec<Value> = settings.iter().map(|(name, kind)| json!({"name": name, "kind": kind})).collect();
        let had = node.get("promoted").and_then(Value::as_array).cloned().unwrap_or_default();
        promoted.extend(had.into_iter().filter(|p| {
            let name = p.get("name").and_then(Value::as_str);
            !settings.iter().any(|(s, _)| Some(*s) == name)
        }));
        node["promoted"] = Value::Array(promoted);
    }
}

// --- 5 ----------------------------------------------------------------------

const RENAMED_NODES: [(&str, &str); 8] = [
    ("ADSR", "Envelope"),
    ("Position Merge", "Pan Tilt to Position"),
    ("Position Split", "Position to Pan Tilt"),
    ("Latch", "Tick Delay"),
    ("Flip-Flop", "SR Flip-Flop"),
    ("Clock", "Link Clock"),
    ("Scaler", "Range to Unit"),
    ("Bar Loop", "Bar"),
];

/// (node, old name, new name). Promoted inputs and snapshot values refer to a
/// setting by name.
const RENAMED_SETTINGS: [(&str, &str, &str); 7] = [
    ("Lookup Table", "Input", "Select by"),
    ("Fill", "Intensity", "amount"),
    ("Bar", "Intensity", "amount"),
    ("Wave Loop", "Intensity", "amount"),
    ("Ripple Burst", "Intensity", "amount"),
    ("Spark Burst", "Intensity", "amount"),
    ("Ripple Burst", "Decay", "Lifetime"),
];

/// The name a node type has at version 5, as one of the names the steps
/// compare against; `None` for a type no step is about.
fn name_at_5(saved: &str) -> Option<&'static str> {
    const KNOWN: [&str; 15] = [
        "Input Controller", "Audio Input", "Launchpad S", "Push 1", "X1", "Change Detect", "Envelope", "Transition",
        "Range to Unit", "Scale Offset", "Bipolar", "Counter", "Phase Scaler", "Palette to Gradient", "Trigger Hold",
    ];
    let renamed = RENAMED_NODES.iter().find(|(old, _)| *old == saved).map(|(_, new)| *new);
    renamed.or_else(|| KNOWN.iter().find(|k| **k == saved).copied())
}

fn current_names(level: &mut Value) {
    for node in nodes_mut(level) {
        let Some(saved) = node.get("type_name").and_then(Value::as_str).map(str::to_string) else { continue };

        // Sin and Cos became the LFO driven by its phase input: waveform
        // (param 1) Sine = 0 or Cosine = 6, "Phase input" (param 3) on. Their
        // amplitude input is the LFO's Amplitude, promoted onto the same port.
        if saved == "Sin" || saved == "Cos" {
            node["type_name"] = json!("LFO");
            node["params"] = json!([
                {"index": 1, "value": if saved == "Cos" { 6 } else { 0 }},
                {"index": 3, "value": true},
            ]);
            node["promoted"] = json!([{"name": "Amplitude", "kind": "Float"}]);
            continue;
        }

        let type_name = RENAMED_NODES.iter().find(|(old, _)| *old == saved).map_or(saved.as_str(), |(_, new)| new);
        if type_name != saved {
            node["type_name"] = json!(type_name);
        }

        let setting = |name: &mut Value| {
            let renamed = RENAMED_SETTINGS.iter().find(|(n, old, _)| *n == type_name && name.as_str() == Some(old));
            if let Some((_, _, new)) = renamed {
                *name = json!(new);
            }
        };
        if let Some(promoted) = node.get_mut("promoted").and_then(Value::as_array_mut) {
            promoted.iter_mut().filter_map(|p| p.get_mut("name")).for_each(setting);
        }
        // A snapshot's values are [name, value] pairs.
        let snapshots = node.pointer_mut("/snapshots/snapshots").and_then(Value::as_array_mut);
        for snapshot in snapshots.into_iter().flatten() {
            let values = snapshot.get_mut("values").and_then(Value::as_array_mut);
            values.into_iter().flatten().filter_map(|pair| pair.get_mut(0)).for_each(setting);
        }
    }
}

// --- 6 ----------------------------------------------------------------------

/// The nodes that gained a bus output, and the output index it sits at: every
/// output from there on moved one place. By the node's name since version 5.
const BUS_OUTPUT_AT: [(&str, u64); 5] = [
    // After "connected".
    ("Launchpad S", 1),
    ("Button Group", 0),
    ("Fader Group", 0),
    ("Trigger Bank", 0),
    ("Toggle Bank", 0),
];

fn bus_outputs_first(level: &mut Value) {
    let types: HashMap<u64, String> = level
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|n| Some((n.get("id")?.as_u64()?, n.get("type_name")?.as_str()?.to_string())))
        .collect();
    for c in connections_mut(level) {
        let (Some(node), Some(port)) = (c.get("from_node").and_then(Value::as_u64), c.get("from_port").and_then(Value::as_u64))
        else {
            continue;
        };
        let at = types.get(&node).and_then(|t| BUS_OUTPUT_AT.iter().find(|(name, _)| name == t));
        if let Some((_, at)) = at
            && port >= *at
        {
            c["from_port"] = Value::from(port + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgraded(mut level: Value) -> Value {
        SCHEMA.upgrade(&mut level).unwrap();
        level
    }

    fn node(type_name: &str, id: u64) -> Value {
        json!({"type_name": type_name, "id": id, "pos": [0.0, 0.0], "params": []})
    }

    fn wire(from: (u64, u64), to: (u64, u64)) -> Value {
        json!({"from_node": from.0, "from_port": from.1, "to_node": to.0, "to_port": to.1})
    }

    /// The (from_port, to_port) of every wire.
    fn ports(level: &Value) -> Vec<(u64, u64)> {
        level["connections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["from_port"].as_u64().unwrap(), c["to_port"].as_u64().unwrap()))
            .collect()
    }

    #[test]
    fn a_current_project_is_left_exactly_as_it_is() {
        let level = json!({
            "version": SCHEMA.current(),
            "nodes": [node("X1", 1), node("ADSR", 2), node("Sin", 3)],
            "connections": [wire((1, 0), (2, 0))],
        });
        assert_eq!(upgraded(level.clone()), level);
    }

    #[test]
    fn hardware_outputs_move_past_the_connected_port_once() {
        let nodes = json!([node("X1", 1), node("Value Display", 2), node("Add", 3)]);
        let wires = json!([wire((1, 0), (2, 0)), wire((1, 4), (2, 0)), wire((3, 0), (2, 0))]);
        let old = upgraded(json!({"nodes": nodes, "connections": wires}));
        assert_eq!(ports(&old), [(1, 0), (5, 0), (0, 0)]);
        assert_eq!(old["version"], SCHEMA.current());
        let v1 = upgraded(json!({"version": 1, "nodes": nodes, "connections": wires}));
        assert_eq!(ports(&v1), [(0, 0), (4, 0), (0, 0)]);
    }

    #[test]
    fn change_detect_and_envelope_inputs_swap() {
        let level = |version: u32, type_name: &str| {
            upgraded(json!({
                "version": version,
                "nodes": [node(type_name, 1), node("Add", 2)],
                "connections": [wire((2, 0), (1, 0)), wire((2, 0), (1, 1))],
            }))
        };
        assert_eq!(ports(&level(1, "Change Detect")), [(0, 1), (0, 0)]);
        assert_eq!(ports(&level(2, "Change Detect")), [(0, 0), (0, 1)]);
        assert_eq!(ports(&level(2, "Envelope")), [(0, 1), (0, 0)]);
        // A file that old may still call the Envelope "ADSR".
        let adsr = level(2, "ADSR");
        assert_eq!(ports(&adsr), [(0, 1), (0, 0)]);
        assert_eq!(adsr["nodes"][0]["type_name"], "Envelope");
        assert_eq!(ports(&level(3, "Envelope")), [(0, 0), (0, 1)]);
    }

    #[test]
    fn transition_gets_its_signal_first_unless_it_is_phase_out() {
        let level = |mode: Value| {
            let mut transition = node("Transition", 1);
            transition["params"] = json!([{"index": 0, "value": mode}]);
            upgraded(json!({
                "version": 2,
                "nodes": [transition, node("Add", 2)],
                "connections": [wire((2, 0), (1, 0)), wire((2, 0), (1, 1)), wire((2, 0), (1, 2))],
            }))
        };
        assert_eq!(ports(&level(json!(1))), [(0, 1), (0, 2), (0, 0)]);
        assert_eq!(ports(&level(json!(4))), [(0, 0), (0, 1), (0, 2)]);
        assert_eq!(ports(&level(json!(4.0))), [(0, 0), (0, 1), (0, 2)]);
    }

    #[test]
    fn override_ports_become_the_leading_promoted_inputs() {
        let mut scale_offset = node("Scale Offset", 1);
        scale_offset["promoted"] = json!([{"name": "Offset", "kind": "Float"}, {"name": "Invert", "kind": "Bool"}]);
        let out = upgraded(json!({
            "version": 3,
            "nodes": [scale_offset, node("Scaler", 2), node("Add", 3)],
            "connections": [wire((3, 0), (1, 2))],
        }));
        assert_eq!(
            out["nodes"][0]["promoted"],
            json!([{"name": "Scale", "kind": "Float"}, {"name": "Offset", "kind": "Float"}, {"name": "Invert", "kind": "Bool"}])
        );
        assert_eq!(ports(&out), [(0, 2)], "the wire stays on the third port, now the promoted Offset");
        // A node saved under its old name is recognised too.
        assert_eq!(out["nodes"][1]["type_name"], "Range to Unit");
        assert_eq!(out["nodes"][1]["promoted"], json!([{"name": "In Min", "kind": "Float"}, {"name": "In Max", "kind": "Float"}]));
        assert!(out["nodes"][2].get("promoted").is_none());

        // From version 4 on the list is the user's, also when it is empty.
        let v4 = upgraded(json!({"version": 4, "nodes": [node("Scale Offset", 1)], "connections": []}));
        assert!(v4["nodes"][0].get("promoted").is_none());
    }

    #[test]
    fn sin_and_cos_become_lfos_on_their_phase_input() {
        let out = upgraded(json!({
            "version": 4,
            "nodes": [node("Sin", 1), node("Cos", 2)],
            "connections": [wire((1, 0), (2, 1))],
        }));
        for (i, waveform) in [(0, 0), (1, 6)] {
            let n = &out["nodes"][i];
            assert_eq!(n["type_name"], "LFO");
            assert_eq!(n["params"], json!([{"index": 1, "value": waveform}, {"index": 3, "value": true}]));
            assert_eq!(n["promoted"], json!([{"name": "Amplitude", "kind": "Float"}]));
        }
        assert_eq!(ports(&out), [(0, 1)]);
    }

    #[test]
    fn renamed_settings_follow_in_promoted_inputs_and_snapshots() {
        let mut ripple = node("Ripple Burst", 1);
        ripple["promoted"] = json!([{"name": "Decay", "kind": "Float"}, {"name": "Speed", "kind": "Float"}]);
        ripple["snapshots"] = json!({"snapshots": [{"id": 1, "name": "a", "values": [["Intensity", {"Float": 0.5}], ["Speed", {"Float": 1.0}]]}]});
        // Bar was "Bar Loop" when its setting was still "Intensity".
        let mut bar = node("Bar Loop", 2);
        bar["promoted"] = json!([{"name": "Intensity", "kind": "Float"}]);
        // The same setting name on another node is not touched.
        let mut envelope = node("Envelope", 3);
        envelope["promoted"] = json!([{"name": "Decay", "kind": "Float"}]);

        let out = upgraded(json!({"version": 4, "nodes": [ripple, bar, envelope], "connections": []}));
        assert_eq!(out["nodes"][0]["promoted"], json!([{"name": "Lifetime", "kind": "Float"}, {"name": "Speed", "kind": "Float"}]));
        assert_eq!(out["nodes"][0]["snapshots"]["snapshots"][0]["values"][0][0], "amount");
        assert_eq!(out["nodes"][0]["snapshots"]["snapshots"][0]["values"][1][0], "Speed");
        assert_eq!(out["nodes"][1]["type_name"], "Bar");
        assert_eq!(out["nodes"][1]["promoted"][0]["name"], "amount");
        assert_eq!(out["nodes"][2]["promoted"][0]["name"], "Decay");
    }

    #[test]
    fn the_bus_output_pushes_the_outputs_after_it_one_place_on() {
        let level = |version: u32| {
            upgraded(json!({
                "version": version,
                "nodes": [node("Launchpad S", 1), node("Fader Group", 2), node("Toggle Bank", 3), node("Add", 4), node("Value Display", 5)],
                // connected, a pad, a Fader Group cell, a Toggle Bank cell, and an Add's output.
                "connections": [wire((1, 0), (5, 0)), wire((1, 3), (5, 0)), wire((2, 0), (5, 0)), wire((3, 1), (5, 0)), wire((4, 0), (5, 0))],
            }))
        };
        // "connected" stays, everything else on the hardware moves; Add does not.
        assert_eq!(ports(&level(5)), [(0, 0), (4, 0), (1, 0), (2, 0), (0, 0)]);
        // A file that is current is left alone.
        assert_eq!(ports(&level(6)), [(0, 0), (3, 0), (0, 0), (1, 0), (0, 0)]);
    }

    #[test]
    fn inner_graphs_are_upgraded_too() {
        let mut subgraph = node("Subgraph", 1);
        subgraph["inner_graph"] = json!({
            "version": 1,
            "nodes": [node("Change Detect", 5), node("Latch", 6)],
            "connections": [wire((6, 0), (5, 0))],
        });
        let mut root = json!({"version": SCHEMA.current(), "nodes": [subgraph], "connections": []});
        assert_eq!(SCHEMA.upgrade(&mut root).unwrap(), 1);
        let inner = &root["nodes"][0]["inner_graph"];
        assert_eq!(inner["version"], SCHEMA.current());
        assert_eq!(ports(inner), [(0, 1)]);
        assert_eq!(inner["nodes"][1]["type_name"], "Tick Delay");
    }
}
