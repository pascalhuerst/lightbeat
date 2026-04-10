use egui::{Color32, Ui};

use crate::engine::nodes::io::fixture::FixtureDisplay;
use crate::objects::channel::ChannelKind;
use crate::objects::channel::ColorMode;
use super::nodes::NodeGraph;

/// Collected info for displaying a fixture in the list.
struct FixtureInfo {
    name: String,
    address: String,
    footprint: usize,
    channels: Vec<ChannelInfo>,
}

struct ChannelInfo {
    name: String,
    kind_label: String,
    dmx_count: usize,
}

/// Show the fixture list panel contents.
///
/// Iterates all nodes in the graph, finds Fixture nodes, and displays
/// their name, address, and channel layout.
pub fn show_fixture_list(ui: &mut Ui, graph: &NodeGraph) {
    ui.heading("Fixtures");
    ui.separator();

    let fixtures = collect_fixtures(graph);

    if fixtures.is_empty() {
        ui.colored_label(Color32::from_gray(120), "No fixtures in project.");
        ui.label("Add a Fixture node from the context menu.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, f) in fixtures.iter().enumerate() {
            ui.push_id(i, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new(&f.name).strong(),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_gray(140), "Address:");
                        ui.label(&f.address);
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_gray(140), "Footprint:");
                        ui.label(format!("{} DMX channels", f.footprint));
                    });

                    if !f.channels.is_empty() {
                        ui.add_space(4.0);
                        for ch in &f.channels {
                            ui.horizontal(|ui| {
                                ui.label("  ");
                                ui.label(&ch.name);
                                ui.colored_label(
                                    Color32::from_gray(100),
                                    format!("({}, {}ch)", ch.kind_label, ch.dmx_count),
                                );
                            });
                        }
                    }
                });
            });
        }
    });
}

fn collect_fixtures(graph: &NodeGraph) -> Vec<FixtureInfo> {
    let mut fixtures = Vec::new();

    for node in graph.all_nodes() {
        if node.type_name() != "Fixture" {
            continue;
        }

        let shared = node.shared_state().lock().unwrap();
        let display = shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<FixtureDisplay>());

        if let Some(d) = display {
            let f = &d.fixture;
            fixtures.push(FixtureInfo {
                name: f.name.clone(),
                address: format!(
                    "{}.{}.{} ch{}",
                    f.address.net, f.address.subnet, f.address.universe, f.address.start_channel
                ),
                footprint: f.dmx_footprint(),
                channels: f
                    .channels
                    .iter()
                    .map(|ch| ChannelInfo {
                        name: ch.name.clone(),
                        kind_label: kind_label(&ch.kind).to_string(),
                        dmx_count: ch.kind.dmx_channel_count(),
                    })
                    .collect(),
            });
        }
    }

    fixtures
}

fn kind_label(kind: &ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Dimmer => "Dimmer",
        ChannelKind::Color { mode } => match mode {
            ColorMode::Rgb => "RGB",
            ColorMode::Rgbw { .. } => "RGBW",
            ColorMode::Cmy => "CMY",
            ColorMode::Hs => "H/S",
        },
        ChannelKind::PanTilt { fine } => {
            if *fine { "Pan/Tilt 16bit" } else { "Pan/Tilt" }
        }
        ChannelKind::Raw { .. } => "Raw",
    }
}
