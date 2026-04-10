use egui::Ui;

use super::nodes::{NodeWidget, ParamDef, ParamValue};

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
