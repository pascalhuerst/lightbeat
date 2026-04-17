use std::any::Any;

use egui::{self, Color32, Pos2, Stroke, StrokeKind, Ui};

use crate::audio::analyzers::AnalyzerKind;
use crate::audio::manager::{AudioInputManager, SharedAudioInputs};
use crate::engine::nodes::io::audio_input::AudioInputDisplay;
use crate::engine::types::*;
use crate::widgets::fader::{self, FaderStyle};
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;
use crate::widgets::nodes::ui::peak_meter::{self, MeterState};

/// Number of envelope samples kept in the rolling buffer. With UI repaint
/// at ~60 fps this works out to ~5 s of history; if the frame rate dips,
/// the visible time stretches a little — that's fine for a glance display.
const ENVELOPE_BUFFER_SIZE: usize = 300;
const ENVELOPE_PLOT_HEIGHT: f32 = 36.0;
/// Reserved gap on the right so the plot's effective width matches the peak
/// meter's bar (which leaves space for the clip indicator on the right).
const PLOT_RIGHT_GAP: f32 = 16.0;

pub struct AudioInputWidget {
    id: NodeId,
    shared: SharedState,
    input_id: u32,
    /// Mirror from engine display.
    outputs: Vec<(String, PortType, f32)>,
    /// (kind, values) per analyzer — drives the inline mini-visualisations.
    analyzer_results: Vec<(AnalyzerKind, Vec<f32>)>,
    /// Per-analyzer-index UI state for inline meters (peak hold, clip latch).
    /// Indices align with `analyzer_results`; non-PeakLevel slots are unused.
    meter_state: Vec<MeterState>,
    /// Per-analyzer rolling envelope history (just values; index in the
    /// vec maps linearly to time across the plot width).
    envelope_history: Vec<Vec<f32>>,
    audio: SharedAudioInputs,
    /// Last input_id we registered as bound — used to update the binding
    /// when the engine display reveals a load-time input_id we didn't pick.
    bound_input: u32,
}

impl AudioInputWidget {
    pub fn new(id: NodeId, shared: SharedState, audio: SharedAudioInputs) -> Self {
        Self {
            id, shared, input_id: 0,
            outputs: Vec::new(),
            analyzer_results: Vec::new(),
            meter_state: Vec::new(),
            envelope_history: Vec::new(),
            audio, bound_input: 0,
        }
    }

    fn push_config(&self) {
        let mut shared = self.shared.lock().unwrap();
        shared.pending_config = Some(serde_json::json!({ "input_id": self.input_id }));
    }

    fn rebind_to(&mut self, new_id: u32) {
        if new_id == self.bound_input { return; }
        AudioInputManager::rebind(&self.audio, self.id.0, new_id);
        self.bound_input = new_id;
    }

    /// Pre-populate `input_id` and the output-port list from save_data + the
    /// live audio-inputs registry, so connections survive the first frame's
    /// `cleanup_stale_connections` after a project load.
    pub fn restore_from_save_data(&mut self, data: &serde_json::Value) {
        if let Some(id) = data.get("input_id").and_then(|v| v.as_u64()) {
            self.input_id = id as u32;
        }
        if self.input_id == 0 { return; }
        let state = self.audio.lock().unwrap();
        if let Some(input) = state.iter().find(|c| c.id == self.input_id) {
            // Mirror the engine's port layout (matches
            // AudioInputProcessNode::process): per-analyzer prefixed ports.
            let mut outs = Vec::new();
            for (i, kind) in input.analyzer_kinds.iter().enumerate() {
                for p in crate::audio::analyzers::AnalyzerInstance::outputs_for_kind(*kind) {
                    outs.push((format!("a{}.{}", i, p.name), p.port_type, 0.0));
                }
            }
            self.outputs = outs;
        }
    }
}

impl Drop for AudioInputWidget {
    fn drop(&mut self) {
        AudioInputManager::release(&self.audio, self.id.0);
    }
}

impl NodeWidget for AudioInputWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Audio Input" }
    fn title(&self) -> &str { "Audio Input" }
    fn description(&self) -> &'static str {
        "Outputs analyzer values from the selected audio input. Add analyzers in the Audio Inputs window."
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> { vec![] }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        self.outputs.iter().map(|(name, ty, _)| {
            UiPortDef::from_def(&PortDef::new(name.clone(), *ty))
        }).collect()
    }

    fn min_width(&self) -> f32 { 180.0 }
    fn min_content_height(&self) -> f32 {
        // Per-analyzer mini-row heights vary by kind: envelope plot is the
        // tallest, the others are single-line.
        let mut h = 24.0_f32;
        for (kind, _) in &self.analyzer_results {
            h += match kind {
                AnalyzerKind::Envelope => ENVELOPE_PLOT_HEIGHT + 4.0,
                _ => 18.0,
            };
        }
        h
    }
    fn resizable(&self) -> bool { true }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        let snapshot = {
            let shared = self.shared.lock().unwrap();
            shared.display.as_ref()
                .and_then(|d| d.downcast_ref::<AudioInputDisplay>())
                .map(|d| (d.input_id, d.input_name.clone(), d.outputs.clone(), d.analyzer_results.clone()))
        };
        if let Some((id, _name, outs, results)) = snapshot {
            self.input_id = id;
            self.outputs = outs;
            self.analyzer_results = results;
            self.rebind_to(id);
        }

        if self.input_id == 0 {
            ui.colored_label(Color32::from_gray(120), "No input selected");
            return;
        }
        if self.analyzer_results.is_empty() {
            ui.colored_label(Color32::from_gray(120), "No analyzers");
            return;
        }

        // Resize per-analyzer meter state and envelope history.
        if self.meter_state.len() != self.analyzer_results.len() {
            self.meter_state.resize(self.analyzer_results.len(), MeterState::default());
        }
        if self.envelope_history.len() != self.analyzer_results.len() {
            self.envelope_history.resize(self.analyzer_results.len(), Vec::new());
        }
        let now = ui.ctx().input(|i| i.time);

        let style = FaderStyle::default();
        for (i, (kind, vals)) in self.analyzer_results.iter().enumerate() {
            match kind {
                AnalyzerKind::Beat => {
                    let bpm = vals.get(1).copied().unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_gray(120), format!("a{}", i));
                        if bpm > 0.0 {
                            ui.colored_label(Color32::from_gray(200), format!("{:.1} BPM", bpm));
                        } else {
                            ui.colored_label(Color32::from_gray(120), "BPM —");
                        }
                    });
                }
                AnalyzerKind::PeakLevel => {
                    let peak = vals.first().copied().unwrap_or(0.0);
                    let rms = vals.get(1).copied().unwrap_or(0.0);
                    self.meter_state[i].tick(peak, now);
                    let state = self.meter_state[i];
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_gray(120), format!("a{}", i));
                        let avail = ui.available_size();
                        let bar_size = egui::Vec2::new(avail.x.max(60.0), 12.0);
                        let (resp, painter) = ui.allocate_painter(bar_size, egui::Sense::hover());
                        peak_meter::draw_horizontal(
                            &painter, resp.rect, peak, rms,
                            state.peak_hold, state.clipping(now),
                        );
                    });
                }
                AnalyzerKind::Envelope => {
                    let env = vals.first().copied().unwrap_or(0.0);

                    // Push current value, drop oldest if over capacity.
                    let history = &mut self.envelope_history[i];
                    history.push(env);
                    if history.len() > ENVELOPE_BUFFER_SIZE {
                        let drop = history.len() - ENVELOPE_BUFFER_SIZE;
                        history.drain(..drop);
                    }

                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_gray(120), format!("a{}", i));
                        let avail = ui.available_size();
                        // Match peak-meter width: leave a gap on the right
                        // (where the peak meter places its clip indicator).
                        let plot_w = (avail.x - PLOT_RIGHT_GAP).max(60.0);
                        let plot_size = egui::Vec2::new(plot_w, ENVELOPE_PLOT_HEIGHT);
                        let (resp, painter) = ui.allocate_painter(plot_size, egui::Sense::hover());
                        draw_envelope_plot(&painter, resp.rect, history);
                    });
                }
            }
        }
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        // Tuple: (id, name, available_to_us). Available = unbound or bound to self.
        let inputs: Vec<(u32, String, bool)> = {
            let state = self.audio.lock().unwrap();
            state.iter().map(|c| {
                let avail = match c.bound_to {
                    None => true,
                    Some(other) => other == self.id.0,
                };
                (c.id, c.name.clone(), avail)
            }).collect()
        };

        ui.horizontal(|ui| {
            ui.label("Audio input:");
            let current = inputs.iter()
                .find(|(id, _, _)| *id == self.input_id)
                .map(|(_, n, _)| n.clone())
                .unwrap_or_else(|| "(none)".to_string());
            egui::ComboBox::from_id_salt(("ai_pick", self.id))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(self.input_id == 0, "(none)").clicked() {
                        self.input_id = 0;
                        self.rebind_to(0);
                        self.push_config();
                    }
                    for (id, name, avail) in &inputs {
                        // Hide inputs already bound to a different node.
                        if !avail { continue; }
                        if ui.selectable_label(self.input_id == *id, name).clicked() {
                            self.input_id = *id;
                            self.rebind_to(*id);
                            self.push_config();
                        }
                    }
                });
        });

        if !self.outputs.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Live Outputs").strong());
            for (name, _ty, value) in &self.outputs {
                ui.horizontal(|ui| {
                    ui.label(name);
                    ui.colored_label(Color32::from_gray(180), format!("{:.2}", value));
                });
            }
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

const ENV_BG: Color32 = Color32::from_gray(20);
const ENV_BORDER: Color32 = Color32::from_gray(60);
const ENV_LINE: Color32 = Color32::from_rgb(80, 200, 160);

/// Plot the rolling envelope buffer across `rect`. Samples are drawn evenly
/// spaced with index 0 at the left and the latest sample at the right edge
/// (or earlier when the buffer hasn't filled yet).
fn draw_envelope_plot(painter: &egui::Painter, rect: egui::Rect, history: &[f32]) {
    painter.rect_filled(rect, 2.0, ENV_BG);
    painter.rect_stroke(rect, 2.0, Stroke::new(1.0, ENV_BORDER), StrokeKind::Inside);

    if history.len() < 2 { return; }

    let w = rect.width();
    let h = rect.height();
    let bottom = rect.max.y;
    let cap = ENVELOPE_BUFFER_SIZE.max(2) as f32 - 1.0;

    let mut prev: Option<Pos2> = None;
    for (i, &v) in history.iter().enumerate() {
        let x = rect.min.x + (i as f32 / cap) * w;
        let y = bottom - v.clamp(0.0, 1.0) * h;
        let pos = Pos2::new(x, y);
        if let Some(p) = prev {
            painter.line_segment([p, pos], Stroke::new(1.5, ENV_LINE));
        }
        prev = Some(pos);
    }
}
