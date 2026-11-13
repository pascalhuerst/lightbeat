use egui::{self, Ui};

use super::nodes::node::NodeWidget;
use super::nodes::types::PortTypeUi;
use crate::engine::types::{ParamDef, ParamValue, PortDef, PortType, PromotedKind, PromotedParam};
use crate::theme;

/// Read params from a node's shared state.
fn read_params(node: &dyn NodeWidget) -> Vec<ParamDef> {
    node.shared_state().lock().unwrap().current_params.clone()
}

/// What a double-click on `param` sets it to: the value it had when the node
/// was made. None when the node does not know (its parameters are made up
/// later) or the setting already is the default.
fn reset_value(node: &dyn NodeWidget, param: &ParamDef) -> Option<ParamValue> {
    let shared = node.shared_state().lock().unwrap();
    let default = shared
        .default_params
        .iter()
        .find(|d| d.name() == param.name() && std::mem::discriminant(*d) == std::mem::discriminant(param))?;
    let differs = format!("{:?}", default.as_value()) != format!("{:?}", param.as_value());
    differs.then(|| default.as_value())
}

/// Put the setting `name` of every node back to its default.
fn reset_all(nodes: &[&mut Box<dyn NodeWidget>], name: &str) {
    for node in nodes {
        let params = read_params(node.as_ref());
        if let Some((i, param)) = params.iter().enumerate().find(|(_, p)| p.name() == name)
            && let Some(d) = reset_value(node.as_ref(), param)
        {
            push_param(node.as_ref(), i, d);
        }
    }
}

/// A setting's name that resets the setting when double-clicked. Takes the
/// place of `ui.label`.
fn param_label(ui: &mut Ui, text: &str, reset: Option<ParamValue>) -> Option<ParamValue> {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    if reset.is_some() {
        response.clone().on_hover_text("Double-click to reset");
    }
    if response.double_clicked() { reset } else { None }
}

/// Push a param change into a node's shared state for the engine to pick up.
fn push_param(node: &dyn NodeWidget, index: usize, value: ParamValue) {
    node.shared_state()
        .lock()
        .unwrap()
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
        let shared = node.shared_state().lock().unwrap();
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
    matches!(param, ParamDef::Choice { name, .. } if name == "Mode")
}

/// Changing a mode leaves the exposed settings meaningless (a name can mean
/// another thing in the next mode), so they go, with their wires. The snapshot
/// inputs are not settings and stay.
fn drop_exposed_inputs(node: &dyn NodeWidget) {
    let mut shared = node.shared_state().lock().unwrap();
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
    if promo.non_promotable.contains(&name) || is_mode(param) {
        return;
    }
    let mut on = promo.slot(name).is_some();
    let hint = "Expose this setting as an input port. While a wire is connected it controls the value.";
    if ui.checkbox(&mut on, "in").on_hover_text(hint).changed() {
        let mut shared = node.shared_state().lock().unwrap();
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

/// Draw the inspector panel for a single selected node.
pub fn show_inspector(ui: &mut Ui, node: &mut dyn NodeWidget) {
    let title = node.title();
    let type_name = node.type_name();
    // User-set custom title takes the big heading slot; an empty title means
    // "no custom name" so we skip the heading entirely.
    if !title.is_empty() {
        ui.heading(title);
    }
    // The node type is always shown in its own muted slot so every inspector
    // header has the same visual anchor — "what kind of thing is this".
    ui.colored_label(
        theme::TEXT_DIM,
        egui::RichText::new(type_name).small().italics(),
    );
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
            ui.label(egui::RichText::new("Inputs").strong().size(11.0));
            let shared = node.shared_state().lock().unwrap();
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
            ui.label(egui::RichText::new("Outputs").strong().size(11.0));
            let shared = node.shared_state().lock().unwrap();
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
        ui.label(egui::RichText::new("Parameters").strong().size(11.0));
        ui.add_space(4.0);

        let promo = PromoteCtx::read(node);
        let mut last_group: Option<String> = None;
        for (i, param) in params.iter().enumerate() {
            if hidden.contains(&i) { continue; }
            let (group, label) = node.param_display(param.name());
            if group != last_group {
                if let Some(g) = &group {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(g).strong());
                }
                last_group = group;
            }
            // A wired promoted param is driven by its signal; editing it by
            // hand would only be overwritten on the next tick.
            let locked = promo.driven(param.name());
            match param {
                ParamDef::Float {
                    name: _,
                    value,
                    min,
                    max,
                    step,
                    unit,
                } => {
                    let mut v = *value;
                    let default = reset_value(node, param);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!locked, |ui| {
                            let mut reset = param_label(ui, &label, default.clone());
                            let slider = egui::Slider::new(&mut v, *min..=*max)
                                .step_by(*step as f64)
                                .suffix(*unit);
                            let response = ui.add(slider);
                            if response.changed() {
                                push_param(node, i, ParamValue::Float(v));
                            }
                            if response.double_clicked() {
                                reset = default;
                            }
                            if let Some(d) = reset {
                                push_param(node, i, d);
                            }
                        });
                        promote_checkbox(ui, node, &promo, param);
                    });
                }
                ParamDef::Int {
                    name: _,
                    value,
                    min,
                    max,
                } => {
                    let mut v = *value;
                    let default = reset_value(node, param);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!locked, |ui| {
                            let mut reset = param_label(ui, &label, default.clone());
                            let slider = egui::Slider::new(&mut v, *min..=*max);
                            let response = ui.add(slider);
                            if response.changed() {
                                push_param(node, i, ParamValue::Int(v));
                            }
                            if response.double_clicked() {
                                reset = default;
                            }
                            if let Some(d) = reset {
                                push_param(node, i, d);
                            }
                        });
                        promote_checkbox(ui, node, &promo, param);
                    });
                }
                ParamDef::Bool { name: _, value } => {
                    let mut v = *value;
                    let default = reset_value(node, param);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!locked, |ui| {
                            let response = ui.checkbox(&mut v, &label);
                            if response.changed() {
                                push_param(node, i, ParamValue::Bool(v));
                            }
                            if let Some(d) = default {
                                response.clone().on_hover_text("Double-click to reset");
                                if response.double_clicked() {
                                    push_param(node, i, d);
                                }
                            }
                        });
                        promote_checkbox(ui, node, &promo, param);
                    });
                }
                ParamDef::Choice {
                    name: _,
                    value,
                    options,
                } => {
                    let mut v = *value;
                    let default = reset_value(node, param);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!locked, |ui| {
                            if let Some(d) = param_label(ui, &label, default) {
                                push_param(node, i, d);
                            }
                            egui::ComboBox::from_id_salt(format!("param_{}", i))
                                .selected_text(&options[v])
                                .show_ui(ui, |ui| {
                                    for (oi, opt) in options.iter().enumerate() {
                                        if ui.selectable_value(&mut v, oi, opt).changed() {
                                            push_param(node, i, ParamValue::Choice(v));
                                            if is_mode(param) || promo.non_promotable.contains(&param.name()) {
                                                drop_exposed_inputs(node);
                                            }
                                        }
                                    }
                                });
                        });
                        promote_checkbox(ui, node, &promo, param);
                    });
                }
            }
        }
    }

    ui.add_space(8.0);

    // Custom inspector content (e.g. scope waveform)
    node.show_inspector(ui);
}

/// Find params that are common across all selected nodes (same name, same type).
fn find_common_params(nodes: &[&dyn NodeWidget]) -> Vec<(String, ParamDef)> {
    if nodes.is_empty() {
        return vec![];
    }
    let first_params = read_params(nodes[0]);
    let mut common = Vec::new();

    for param in &first_params {
        let name = param.name().to_string();
        let all_have = nodes[1..].iter().all(|n| {
            read_params(*n)
                .iter()
                .any(|p| p.name() == name && std::mem::discriminant(p) == std::mem::discriminant(param))
        });
        if all_have {
            common.push((name, param.clone()));
        }
    }
    common
}

/// Inspector for multiple selected nodes — shows and edits common parameters.
pub fn show_multi_inspector(ui: &mut Ui, nodes: Vec<&mut Box<dyn NodeWidget>>) {
    let common =
        find_common_params(&nodes.iter().map(|n| &***n).collect::<Vec<&dyn NodeWidget>>());

    if common.is_empty() {
        ui.label("No common parameters.");
        return;
    }

    ui.label(egui::RichText::new("Common Parameters").strong().size(11.0));
    ui.add_space(4.0);

    for (name, param) in &common {
        match param {
            ParamDef::Float {
                value,
                min,
                max,
                step,
                unit,
                ..
            } => {
                let mut v = *value;
                ui.horizontal(|ui| {
                    let label_reset = param_label(ui, name, Some(ParamValue::Bool(true))).is_some();
                    let slider = egui::Slider::new(&mut v, *min..=*max)
                        .step_by(*step as f64)
                        .suffix(*unit);
                    let response = ui.add(slider);
                    if label_reset || response.double_clicked() {
                        reset_all(&nodes, name);
                    }
                    if response.changed() {
                        for node in &nodes {
                            let idx = read_params(node.as_ref())
                                .iter()
                                .position(|p| p.name() == name);
                            if let Some(i) = idx {
                                push_param(node.as_ref(), i, ParamValue::Float(v));
                            }
                        }
                    }
                });
            }
            ParamDef::Int {
                value, min, max, ..
            } => {
                let mut v = *value;
                ui.horizontal(|ui| {
                    let label_reset = param_label(ui, name, Some(ParamValue::Bool(true))).is_some();
                    let slider = egui::Slider::new(&mut v, *min..=*max);
                    let response = ui.add(slider);
                    if label_reset || response.double_clicked() {
                        reset_all(&nodes, name);
                    }
                    if response.changed() {
                        for node in &nodes {
                            let idx = read_params(node.as_ref())
                                .iter()
                                .position(|p| p.name() == name);
                            if let Some(i) = idx {
                                push_param(node.as_ref(), i, ParamValue::Int(v));
                            }
                        }
                    }
                });
            }
            ParamDef::Bool { value, .. } => {
                let mut v = *value;
                if ui.checkbox(&mut v, name).changed() {
                    for node in &nodes {
                        let idx = read_params(node.as_ref())
                            .iter()
                            .position(|p| p.name() == name);
                        if let Some(i) = idx {
                            push_param(node.as_ref(), i, ParamValue::Bool(v));
                        }
                    }
                }
            }
            _ => {}
        }
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
            let mut s = shared.lock().unwrap();
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
}
