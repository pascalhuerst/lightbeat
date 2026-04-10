use egui::Ui;

use super::nodes::{NodeWidget, ParamDef, ParamValue};

/// Find params that are common across all selected nodes (same name, same type).
fn find_common_params(nodes: &[&Box<dyn NodeWidget>]) -> Vec<(String, ParamDef)> {
    if nodes.is_empty() {
        return vec![];
    }
    let first_params = nodes[0].params();
    let mut common = Vec::new();

    for param in &first_params {
        let name = param.name().to_string();
        let all_have = nodes[1..].iter().all(|n| {
            n.params().iter().any(|p| {
                p.name() == name && std::mem::discriminant(p) == std::mem::discriminant(param)
            })
        });
        if all_have {
            common.push((name, param.clone()));
        }
    }
    common
}

/// Inspector for multiple selected nodes — shows and edits common parameters.
pub fn show_multi_inspector(ui: &mut Ui, mut nodes: Vec<&mut Box<dyn NodeWidget>>) {
    let common = find_common_params(&nodes.iter().map(|n| &**n).collect::<Vec<&Box<dyn NodeWidget>>>());

    if common.is_empty() {
        ui.label("No common parameters.");
        return;
    }

    ui.label(egui::RichText::new("Common Parameters").strong().size(11.0));
    ui.add_space(4.0);

    for (name, param) in &common {
        match param {
            ParamDef::Float { value, min, max, step, unit, .. } => {
                let mut v = *value;
                ui.horizontal(|ui| {
                    ui.label(name);
                    let slider = egui::Slider::new(&mut v, *min..=*max)
                        .step_by(*step as f64)
                        .suffix(*unit);
                    if ui.add(slider).changed() {
                        for node in nodes.iter_mut() {
                            let idx = node.params().iter().position(|p| p.name() == name);
                            if let Some(i) = idx {
                                node.set_param(i, ParamValue::Float(v));
                            }
                        }
                    }
                });
            }
            ParamDef::Int { value, min, max, .. } => {
                let mut v = *value;
                ui.horizontal(|ui| {
                    ui.label(name);
                    let slider = egui::Slider::new(&mut v, *min..=*max);
                    if ui.add(slider).changed() {
                        for node in nodes.iter_mut() {
                            let idx = node.params().iter().position(|p| p.name() == name);
                            if let Some(i) = idx {
                                node.set_param(i, ParamValue::Int(v));
                            }
                        }
                    }
                });
            }
            ParamDef::Bool { value, .. } => {
                let mut v = *value;
                if ui.checkbox(&mut v, name).changed() {
                    for node in nodes.iter_mut() {
                        let idx = node.params().iter().position(|p| p.name() == name);
                        if let Some(i) = idx {
                            node.set_param(i, ParamValue::Bool(v));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Draw the inspector panel for a selected node.
pub fn show_inspector(ui: &mut Ui, node: &mut dyn NodeWidget) {
    ui.heading(node.title());
    ui.separator();

    // Ports info
    if !node.inputs().is_empty() {
        ui.label(egui::RichText::new("Inputs").strong().size(11.0));
        for port in node.inputs() {
            ui.horizontal(|ui| {
                let (r, painter) = ui.allocate_painter(
                    egui::Vec2::new(10.0, 10.0),
                    egui::Sense::hover(),
                );
                painter.circle_filled(r.rect.center(), 4.0, port.port_type.color());
                ui.label(&port.name);
            });
        }
        ui.add_space(4.0);
    }

    if !node.outputs().is_empty() {
        ui.label(egui::RichText::new("Outputs").strong().size(11.0));
        for port in node.outputs() {
            ui.horizontal(|ui| {
                let (r, painter) = ui.allocate_painter(
                    egui::Vec2::new(10.0, 10.0),
                    egui::Sense::hover(),
                );
                painter.circle_filled(r.rect.center(), 4.0, port.port_type.color());
                ui.label(&port.name);
            });
        }
        ui.add_space(4.0);
    }

    // Parameters
    let params = node.params();
    if !params.is_empty() {
        ui.separator();
        ui.label(egui::RichText::new("Parameters").strong().size(11.0));
        ui.add_space(4.0);

        for (i, param) in params.iter().enumerate() {
            match param {
                ParamDef::Float { name, value, min, max, step, unit } => {
                    let mut v = *value;
                    ui.horizontal(|ui| {
                        ui.label(name);
                        let slider = egui::Slider::new(&mut v, *min..=*max)
                            .step_by(*step as f64)
                            .suffix(*unit);
                        if ui.add(slider).changed() {
                            node.set_param(i, ParamValue::Float(v));
                        }
                    });
                }
                ParamDef::Int { name, value, min, max } => {
                    let mut v = *value;
                    ui.horizontal(|ui| {
                        ui.label(name);
                        let slider = egui::Slider::new(&mut v, *min..=*max);
                        if ui.add(slider).changed() {
                            node.set_param(i, ParamValue::Int(v));
                        }
                    });
                }
                ParamDef::Bool { name, value } => {
                    let mut v = *value;
                    if ui.checkbox(&mut v, name).changed() {
                        node.set_param(i, ParamValue::Bool(v));
                    }
                }
                ParamDef::Choice { name, value, options } => {
                    let mut v = *value;
                    ui.horizontal(|ui| {
                        ui.label(name);
                        egui::ComboBox::from_id_salt(format!("param_{}", i))
                            .selected_text(&options[v])
                            .show_ui(ui, |ui| {
                                for (oi, opt) in options.iter().enumerate() {
                                    if ui.selectable_value(&mut v, oi, opt).changed() {
                                        node.set_param(i, ParamValue::Choice(v));
                                    }
                                }
                            });
                    });
                }
            }
        }
    }

    ui.add_space(8.0);

    // Custom inspector content (e.g. scope waveform)
    node.show_inspector(ui);
}
