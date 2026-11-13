use egui::{self, Ui};

use super::nodes::node::NodeWidget;
use super::nodes::types::PortTypeUi;
use crate::engine::types::{ParamDef, ParamValue, PortDef, PortType, PromotedKind, PromotedParam};
use crate::theme;
use super::setup_ui;

/// Read params from a node's shared state.
fn read_params(node: &dyn NodeWidget) -> Vec<ParamDef> {
    node.shared_state().lock().current_params.clone()
}

/// What a double-click on `param` sets it to: the value it had when the node
/// was made. None when the node does not know (its parameters are made up
/// later) or the setting already is the default.
fn reset_value(node: &dyn NodeWidget, param: &ParamDef) -> Option<ParamValue> {
    let shared = node.shared_state().lock();
    let default = shared
        .default_params
        .iter()
        .find(|d| d.name() == param.name() && std::mem::discriminant(*d) == std::mem::discriminant(param))?;
    let differs = format!("{:?}", default.as_value()) != format!("{:?}", param.as_value());
    differs.then(|| default.as_value())
}

/// Put the setting `name` of every node back to its default.
fn reset_all(nodes: &[&dyn NodeWidget], name: &str) {
    for node in nodes {
        let params = read_params(*node);
        if let Some((i, param)) = params.iter().enumerate().find(|(_, p)| p.name() == name)
            && let Some(d) = reset_value(*node, param)
        {
            push_param(*node, i, d);
        }
    }
}

/// True when a double-click would change the setting on at least one node.
fn can_reset(nodes: &[&dyn NodeWidget], name: &str) -> bool {
    nodes.iter().any(|node| {
        read_params(*node)
            .iter()
            .find(|p| p.name() == name)
            .is_some_and(|param| reset_value(*node, param).is_some())
    })
}

/// Set the setting `name` on every node, whatever index each keeps it at.
fn set_all(nodes: &[&dyn NodeWidget], name: &str, value: ParamValue) {
    for node in nodes {
        if let Some(i) = read_params(*node).iter().position(|p| p.name() == name) {
            push_param(*node, i, value.clone());
        }
    }
}

/// A setting's name that resets the setting when double-clicked. Takes the
/// place of `ui.label`. Returns true on the double-click.
fn param_label(ui: &mut Ui, text: &str, can_reset: bool) -> bool {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    if can_reset {
        response.clone().on_hover_text("Double-click to reset");
    }
    can_reset && response.double_clicked()
}

/// Push a param change into a node's shared state for the engine to pick up.
fn push_param(node: &dyn NodeWidget, index: usize, value: ParamValue) {
    node.shared_state()
        .lock()
        .pending_params
        .push((index, value));
}

/// Render a port value with type-specific preview.
fn show_port_value(ui: &mut Ui, def: &PortDef, values: &[f32], base: usize) {
    ui.horizontal(|ui| {
        // Port type dot.
        let (r, painter) = ui.allocate_painter(
            egui::Vec2::new(10.0, 10.0),
            egui::Sense::hover(),
        );
        painter.circle_filled(r.rect.center(), 4.0, def.port_type.color());

        ui.label(&def.name);

        // Type-specific value display.
        match def.port_type {
            PortType::Color => {
                let rv = values.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let gv = values.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let bv = values.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                let color = egui::Color32::from_rgb(
                    (rv * 255.0) as u8,
                    (gv * 255.0) as u8,
                    (bv * 255.0) as u8,
                );
                let (cr, cp) = ui.allocate_painter(
                    egui::Vec2::new(32.0, 12.0),
                    egui::Sense::hover(),
                );
                cp.rect_filled(cr.rect, 2.0, color);
                ui.colored_label(
                    theme::TEXT_DIM,
                    format!("#{:02X}{:02X}{:02X}", (rv * 255.0) as u8, (gv * 255.0) as u8, (bv * 255.0) as u8),
                );
            }
            PortType::Position => {
                let pan = values.get(base).copied().unwrap_or(0.0);
                let tilt = values.get(base + 1).copied().unwrap_or(0.0);
                ui.colored_label(
                    theme::TEXT_DIM,
                    format!("P:{:.2} T:{:.2}", pan, tilt),
                );
            }
            PortType::Palette => {
                for slot in 0..4 {
                    let sb = base + slot * 3;
                    let rv = values.get(sb).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    let gv = values.get(sb + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    let bv = values.get(sb + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
                    let color = egui::Color32::from_rgb(
                        (rv * 255.0) as u8, (gv * 255.0) as u8, (bv * 255.0) as u8,
                    );
                    let (cr, cp) = ui.allocate_painter(
                        egui::Vec2::new(14.0, 12.0), egui::Sense::hover(),
                    );
                    cp.rect_filled(cr.rect, 1.0, color);
                }
            }
            _ => {
                let val = values.get(base).copied().unwrap_or(0.0);
                ui.colored_label(theme::TEXT_DIM, format!("{:.2}", val));
            }
        }
    });
}

/// What the inspector needs to know to offer "expose as input" per param.
struct PromoteCtx {
    promoted: Vec<PromotedParam>,
    non_promotable: &'static [&'static str],
    connected: Vec<bool>,
    native_inputs: usize,
}

impl PromoteCtx {
    fn read(node: &dyn NodeWidget) -> Self {
        // `ui_inputs` may lock the node's shared state itself, so it must not
        // be called while the lock below is held.
        let native_inputs = node.ui_inputs().len();
        let shared = node.shared_state().lock();
        Self {
            promoted: shared.promoted.clone(),
            non_promotable: shared.non_promotable,
            connected: shared.inputs_connected.clone(),
            native_inputs,
        }
    }

    fn slot(&self, name: &str) -> Option<usize> {
        self.promoted.iter().position(|p| p.name == name)
    }

    /// True while a wire is feeding this param's input.
    fn driven(&self, name: &str) -> bool {
        self.slot(name)
            .is_some_and(|s| self.connected.get(self.native_inputs + s).copied().unwrap_or(false))
    }
}

/// A dropdown that picks what the node is. It reshapes the node, so a wire may
/// not drive it and changing it drops the inputs the user had added.
fn is_mode(param: &ParamDef) -> bool {
    matches!(param, ParamDef::Choice { name, .. } if name == "Mode" || name == "Type")
}

/// Changing a mode leaves the exposed settings meaningless (a name can mean
/// another thing in the next mode), so they go, with their wires. The snapshot
/// inputs are not settings and stay.
fn drop_exposed_inputs(node: &dyn NodeWidget) {
    let mut shared = node.shared_state().lock();
    let keep = |p: &PromotedParam| {
        matches!(p.kind, PromotedKind::SnapshotSelect | PromotedKind::SnapshotFade)
    };
    if shared.promoted.iter().any(|p| !keep(p)) {
        shared.promoted.retain(keep);
        shared.pending_promoted = Some(shared.promoted.clone());
    }
}

/// Checkbox that turns a param into an input port and back.
fn promote_checkbox(ui: &mut Ui, node: &dyn NodeWidget, promo: &PromoteCtx, param: &ParamDef) {
    let name = param.name();
    let mut on = promo.slot(name).is_some();
    // A project saved before the setting became non-promotable may still have
    // it exposed; the box stays so it can be switched off.
    if (promo.non_promotable.contains(&name) && !on) || is_mode(param) {
        return;
    }
    let hint = "Expose this setting as an input port. While a wire is connected it controls the value.";
    if ui.checkbox(&mut on, "in").on_hover_text(hint).changed() {
        let mut shared = node.shared_state().lock();
        if on {
            shared.promoted.push(PromotedParam {
                name: name.to_string(),
                kind: PromotedKind::of(param),
            });
        } else {
            shared.promoted.retain(|p| p.name != name);
        }
        shared.pending_promoted = Some(shared.promoted.clone());
    }
}

/// A change of this setting reshapes the node (a mode, a type, a slot count),
/// so the settings exposed as inputs no longer mean what they did.
fn reshapes(node: &dyn NodeWidget, param: &ParamDef) -> bool {
    // Switching a node into its sibling (Add to Sub) leaves its other
    // settings what they were.
    if param.name() == OPERATION_PARAM {
        return false;
    }
    is_mode(param) || node.shared_state().lock().non_promotable.contains(&param.name())
}

/// Button and Fader: the checkbox that gives the node an input port.
const INPUT_SWITCH_PARAM: &str = "Input enabled";

/// The setting that turns a node into another of its family (Add, Sub, Mul,
/// Div; the compares; the gates; the constants).
const OPERATION_PARAM: &str = "Operation";

/// One setting as a row. Every selected node gets the change, so the single
/// and the multi-selection inspector draw their settings the same way; only
/// the single one passes `promo` and offers to expose the setting as an input.
fn param_row(
    ui: &mut Ui,
    nodes: &[&dyn NodeWidget],
    param: &ParamDef,
    label: &str,
    promo: Option<&PromoteCtx>,
    locked: bool,
) {
    let name = param.name();
    let can_reset = can_reset(nodes, name);
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!locked, |ui| match param {
            ParamDef::Float { value, min, max, step, unit, .. } => {
                let mut v = *value;
                let mut reset = param_label(ui, label, can_reset);
                let slider = egui::Slider::new(&mut v, *min..=*max).step_by(*step as f64).suffix(*unit);
                let response = ui.add(slider);
                if response.changed() {
                    set_all(nodes, name, ParamValue::Float(v));
                }
                reset |= can_reset && response.double_clicked();
                if reset {
                    reset_all(nodes, name);
                }
            }
            ParamDef::Int { value, min, max, .. } => {
                let mut v = *value;
                let mut reset = param_label(ui, label, can_reset);
                let response = ui.add(egui::Slider::new(&mut v, *min..=*max));
                if response.changed() {
                    set_all(nodes, name, ParamValue::Int(v));
                }
                reset |= can_reset && response.double_clicked();
                if reset {
                    reset_all(nodes, name);
                }
            }
            ParamDef::Bool { value, .. } => {
                let mut v = *value;
                let response = ui.checkbox(&mut v, label);
                if response.changed() {
                    set_all(nodes, name, ParamValue::Bool(v));
                    // The checkbox that adds or removes the node's own input
                    // moves the ports of the exposed settings behind it.
                    if name == INPUT_SWITCH_PARAM {
                        for node in nodes {
                            drop_exposed_inputs(*node);
                        }
                    }
                }
                if can_reset {
                    response.clone().on_hover_text("Double-click to reset");
                    if response.double_clicked() {
                        reset_all(nodes, name);
                    }
                }
            }
            ParamDef::Choice { value, options, .. } => {
                let mut v = *value;
                if param_label(ui, label, can_reset) {
                    reset_all(nodes, name);
                }
                egui::ComboBox::from_id_salt(("param", name))
                    .selected_text(options.get(v).map(|s| s.as_ref()).unwrap_or("?"))
                    .show_ui(ui, |ui| {
                        for (oi, opt) in options.iter().enumerate() {
                            if ui.selectable_value(&mut v, oi, opt.as_ref()).changed() {
                                set_all(nodes, name, ParamValue::Choice(v));
                                for node in nodes {
                                    if reshapes(*node, param) {
                                        drop_exposed_inputs(*node);
                                    }
                                }
                            }
                        }
                    });
            }
        });
        if let Some(promo) = promo {
            promote_checkbox(ui, nodes[0], promo, param);
        }
    });
}

/// Draw the inspector panel for a single selected node.
pub fn show_inspector(ui: &mut Ui, node: &mut dyn NodeWidget) {
    let title = node.title();
    let type_name = node.type_name();
    // User-set custom title takes the big heading slot; an empty title means
    // "no custom name" so we skip the heading entirely.
    if !title.is_empty() {
        ui.heading(title);
    }
    // The node type sits in its own muted slot, "what kind of thing is this",
    // unless the heading already says it (operators are titled by their symbol).
    if title != type_name {
        ui.colored_label(
            theme::TEXT_DIM,
            egui::RichText::new(type_name).small().italics(),
        );
    }
    let desc = node.description();
    if !desc.is_empty() {
        ui.colored_label(theme::TEXT_DIM, desc);
    }
    ui.separator();

    let hide_default_ports = node.inspector_hides_default_ports();

    // Ports info
    if !hide_default_ports {
        let inputs = node.ui_inputs();
        if !inputs.is_empty() {
            setup_ui::section(ui, "Inputs");
            let shared = node.shared_state().lock();
            let values = shared.inputs.clone();
            drop(shared);
            let mut ch_base = 0;
            for port in &inputs {
                let cpe = port.def.port_type.channel_count();
                show_port_value(ui, &port.def, &values, ch_base);
                ch_base += cpe;
            }
            ui.add_space(4.0);
        }

        let outputs = node.ui_outputs();
        if !outputs.is_empty() {
            setup_ui::section(ui, "Outputs");
            let shared = node.shared_state().lock();
            let values = shared.outputs.clone();
            drop(shared);
            let mut ch_base = 0;
            for port in &outputs {
                let cpe = port.def.port_type.channel_count();
                show_port_value(ui, &port.def, &values, ch_base);
                ch_base += cpe;
            }
            ui.add_space(4.0);
        }
    }

    // Parameters
    let params = read_params(node);
    let hidden = node.overridden_param_indices();
    let visible_count = params.iter().enumerate()
        .filter(|(i, _)| !hidden.contains(i)).count();
    if !params.is_empty() && visible_count > 0 {
        ui.separator();
        setup_ui::section(ui, "Parameters");
        ui.add_space(4.0);

        let promo = PromoteCtx::read(node);
        let mut last_group: Option<String> = None;
        for (i, param) in params.iter().enumerate() {
            if hidden.contains(&i) { continue; }
            let (group, label) = node.param_display(param.name());
            if group != last_group {
                if let Some(g) = &group {
                    ui.add_space(6.0);
                    setup_ui::section(ui, g);
                }
                last_group = group;
            }
            // A wired promoted param is driven by its signal; editing it by
            // hand would only be overwritten on the next tick.
            let locked = promo.driven(param.name());
            param_row(ui, &[&*node], param, &label, Some(&promo), locked);
        }
    }

    ui.add_space(8.0);

    // Custom inspector content (e.g. scope waveform)
    node.show_inspector(ui);
}

/// Find params that are common across all selected nodes: the same name and
/// type, and for a dropdown the same options, since an index means nothing
/// otherwise.
fn find_common_params(nodes: &[&dyn NodeWidget]) -> Vec<ParamDef> {
    if nodes.is_empty() {
        return vec![];
    }
    let skipped: Vec<Vec<String>> = nodes
        .iter()
        .map(|n| {
            let params = read_params(*n);
            n.overridden_param_indices().into_iter().filter_map(|i| params.get(i).map(|p| p.name().to_string())).collect()
        })
        .collect();
    let same = |a: &ParamDef, b: &ParamDef| match (a, b) {
        (ParamDef::Choice { options: x, .. }, ParamDef::Choice { options: y, .. }) => x == y,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    };
    read_params(nodes[0])
        .into_iter()
        .filter(|param| {
            let name = param.name();
            skipped.iter().all(|h| !h.iter().any(|n| n == name))
                && nodes[1..].iter().all(|n| read_params(*n).iter().any(|p| p.name() == name && same(p, param)))
        })
        .collect()
}

/// Inspector for multiple selected nodes: shows and edits the settings they
/// share, with the same rows as the single-node inspector. Exposing a setting
/// as an input stays with the single-node inspector, since each node's input
/// list is its own.
pub fn show_multi_inspector(ui: &mut Ui, nodes: Vec<&mut Box<dyn NodeWidget>>) {
    let refs: Vec<&dyn NodeWidget> = nodes.iter().map(|n| &***n).collect();
    let common = find_common_params(&refs);

    if common.is_empty() {
        ui.label("No common parameters.");
        return;
    }

    setup_ui::section(ui, "Common Parameters");
    ui.add_space(4.0);

    for param in &common {
        let (_, label) = refs[0].param_display(param.name());
        param_row(ui, &refs, param, &label, None, false);
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;
    use crate::engine::types::{NodeId, new_shared_state};
    use crate::widgets::nodes::math::gain::GainWidget;

    fn float(value: f32) -> ParamDef {
        ParamDef::Float { name: "Gain".into(), value, min: 0.0, max: 1.0, step: 0.01, unit: "" }
    }

    fn widget(default: Option<f32>, current: f32) -> GainWidget {
        let shared = new_shared_state(1, 1);
        {
            let mut s = shared.lock();
            s.default_params = default.map(|d| vec![float(d)]).unwrap_or_default();
            s.current_params = vec![float(current)];
        }
        GainWidget::new(NodeId(1), shared)
    }

    #[test]
    fn a_changed_setting_resets_to_the_value_the_node_was_made_with() {
        let w = widget(Some(1.0), 0.3);
        let param = float(0.3);
        match reset_value(&w, &param) {
            Some(ParamValue::Float(v)) => assert_eq!(v, 1.0),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nothing_is_offered_when_it_already_is_the_default_or_the_default_is_not_known() {
        assert!(reset_value(&widget(Some(1.0), 1.0), &float(1.0)).is_none());
        assert!(reset_value(&widget(None, 0.3), &float(0.3)).is_none());
    }

    #[test]
    fn a_setting_is_set_and_reset_on_every_selected_node() {
        let (a, b) = (widget(Some(1.0), 0.3), widget(Some(1.0), 0.6));
        let nodes: [&dyn NodeWidget; 2] = [&a, &b];
        assert_eq!(find_common_params(&nodes).len(), 1);
        assert!(can_reset(&nodes, "Gain"));
        set_all(&nodes, "Gain", ParamValue::Float(0.9));
        reset_all(&nodes, "Gain");
        for w in [&a, &b] {
            let pending = w.shared_state().lock().pending_params.clone();
            assert_eq!(pending.len(), 2);
            assert!(matches!(pending[1].1, ParamValue::Float(v) if v == 1.0));
        }
    }
}
