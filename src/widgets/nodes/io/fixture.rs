use std::any::Any;

use egui::{self, Color32, Ui};

use crate::engine::nodes::io::fixture::FixtureDisplay;
use crate::engine::types::*;
use crate::objects::channel::{Channel, ChannelKind, ColorMode};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

pub struct FixtureWidget {
    id: NodeId,
    shared: SharedState,
    pub editor_open: bool,
}

impl FixtureWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self {
            id,
            shared,
            editor_open: false,
        }
    }

    /// Draw the fixture editor window. Call this from the main app update loop.
    pub fn show_editor(&mut self, ctx: &egui::Context) {
        if !self.editor_open {
            return;
        }

        let shared = self.shared.lock().unwrap();
        let fixture = shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<FixtureDisplay>())
            .map(|d| d.fixture.clone());
        drop(shared);

        let Some(fixture) = fixture else { return };

        let mut open = self.editor_open;
        egui::Window::new(format!("Fixture: {}", fixture.name))
            .id(egui::Id::new(("fixture_editor", self.id.0)))
            .open(&mut open)
            .default_size([400.0, 500.0])
            .show(ctx, |ui| {
                // Address section
                ui.heading("DMX Address");
                ui.horizontal(|ui| {
                    ui.label("Address:");
                    ui.label(format!(
                        "{}.{}.{} ch{}",
                        fixture.address.net,
                        fixture.address.subnet,
                        fixture.address.universe,
                        fixture.address.start_channel,
                    ));
                });
                ui.label(format!("Footprint: {} channels", fixture.dmx_footprint()));

                ui.separator();

                // Channel list
                ui.heading("Channels");
                for (i, ch) in fixture.channels.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}.", i + 1));
                        ui.label(&ch.name);
                        ui.colored_label(Color32::from_gray(120), format!("({})", kind_label(&ch.kind)));
                        ui.colored_label(
                            Color32::from_gray(100),
                            format!("{} ch", ch.kind.dmx_channel_count()),
                        );
                    });
                }

                ui.separator();

                // Add channel buttons
                ui.heading("Add Channel");
                ui.horizontal(|ui| {
                    if ui.button("Dimmer").clicked() {
                        let name = format!("Dimmer {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::dimmer(&name));
                    }
                    if ui.button("RGB").clicked() {
                        let name = format!("Color {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::color(&name, ColorMode::Rgb));
                    }
                    if ui.button("RGBW").clicked() {
                        let name = format!("Color {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::color(
                            &name,
                            ColorMode::Rgbw { white_temperature: 6500 },
                        ));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Pan/Tilt").clicked() {
                        let name = format!("PanTilt {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::pan_tilt(&name, false));
                    }
                    if ui.button("Pan/Tilt Fine").clicked() {
                        let name = format!("PanTilt {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::pan_tilt(&name, true));
                    }
                    if ui.button("Raw (1ch)").clicked() {
                        let name = format!("Raw {}", fixture.channels.len() + 1);
                        self.send_add_channel(Channel::raw(&name, 1));
                    }
                });

                if !fixture.channels.is_empty() {
                    ui.separator();
                    if ui.button("Remove last channel").clicked() {
                        self.send_remove_last_channel();
                    }
                }
            });
        self.editor_open = open;
    }

    fn send_add_channel(&self, channel: Channel) {
        // Encode as a special param change the engine fixture node understands.
        if let Ok(json) = serde_json::to_value(&channel) {
            let mut shared = self.shared.lock().unwrap();
            // Use param index 200 as convention for "add channel".
            shared
                .pending_params
                .push((200, ParamValue::Float(0.0))); // placeholder
            // Store the channel data in save_data temporarily for the engine to pick up.
            // Actually, let's use a simpler approach: encode in the pending_params.
            // We'll use index 200+ for fixture-specific commands.
            drop(shared);

            // For now, encode as a JSON string in a special param.
            // The engine fixture node will handle this.
            let mut shared = self.shared.lock().unwrap();
            shared.pending_params.clear(); // clear the placeholder
            // Store channel add request as custom data.
            if let Some(save) = &mut shared.save_data {
                if let Some(obj) = save.as_object_mut() {
                    let mut channels: Vec<serde_json::Value> = obj
                        .get("channels")
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default();
                    channels.push(json);
                    obj.insert("channels".to_string(), serde_json::Value::Array(channels));
                }
            }
        }
    }

    fn send_remove_last_channel(&self) {
        let mut shared = self.shared.lock().unwrap();
        if let Some(save) = &mut shared.save_data {
            if let Some(obj) = save.as_object_mut() {
                if let Some(channels) = obj.get_mut("channels").and_then(|c| c.as_array_mut()) {
                    channels.pop();
                }
            }
        }
    }
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
            if *fine {
                "Pan/Tilt 16bit"
            } else {
                "Pan/Tilt"
            }
        }
        ChannelKind::Raw { .. } => "Raw",
    }
}

impl NodeWidget for FixtureWidget {
    fn node_id(&self) -> NodeId {
        self.id
    }
    fn type_name(&self) -> &'static str {
        "Fixture"
    }
    fn title(&self) -> &str {
        "Fixture"
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![]
    }

    fn min_width(&self) -> f32 {
        130.0
    }
    fn min_content_height(&self) -> f32 {
        40.0
    }

    fn shared_state(&self) -> &SharedState {
        &self.shared
    }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let shared = self.shared.lock().unwrap();
        let display = shared
            .display
            .as_ref()
            .and_then(|d| d.downcast_ref::<FixtureDisplay>());

        if let Some(d) = display {
            ui.label(&d.fixture.name);
            ui.colored_label(
                Color32::from_gray(120),
                format!(
                    "ch{} ({} DMX)",
                    d.fixture.address.start_channel,
                    d.fixture.dmx_footprint()
                ),
            );
        } else {
            ui.label("No fixture");
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        if ui.button("Open Editor").clicked() {
            self.editor_open = true;
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
