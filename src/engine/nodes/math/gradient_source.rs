//! Gradient Source — emits an 8-stop gradient on a single `Gradient` output.
//! The stops are authored in the widget inspector (color picker + position
//! slider + alpha slider per stop) and packed into the 40-channel flat
//! representation `Gradient::to_channels` expects.

use crate::color::{Gradient, GradientStop, Rgb};
use crate::engine::types::*;

const MAX_STOPS: usize = GRADIENT_STOP_COUNT; // 8

pub struct GradientSourceDisplay {
    pub stops: Vec<(f32, Rgb, f32)>, // (position, color, alpha) — only active stops
}

pub struct GradientSourceProcessNode {
    id: NodeId,
    /// Authoritative stops, per-slot. `used[i] = false` means the slot is
    /// skipped at emit time (encoded as alpha = -1 in the output channels).
    stops: [GradientStop; MAX_STOPS],
    used: [bool; MAX_STOPS],
    outputs: Vec<PortDef>,
    channels: [f32; MAX_STOPS * GRADIENT_STOP_FLOATS],
}

impl GradientSourceProcessNode {
    pub fn new(id: NodeId) -> Self {
        // Sensible default: black-to-white linear gradient (2 stops, rest unused).
        let mut stops = [GradientStop::opaque(0.0, Rgb::BLACK); MAX_STOPS];
        let mut used = [false; MAX_STOPS];
        stops[0] = GradientStop::opaque(0.0, Rgb::BLACK);
        stops[1] = GradientStop::opaque(1.0, Rgb::WHITE);
        used[0] = true;
        used[1] = true;
        let mut node = Self {
            id,
            stops,
            used,
            outputs: vec![PortDef::new("gradient", PortType::Gradient)],
            channels: [0.0; MAX_STOPS * GRADIENT_STOP_FLOATS],
        };
        node.recompute_channels();
        node
    }

    fn recompute_channels(&mut self) {
        let live: Vec<GradientStop> = (0..MAX_STOPS)
            .filter(|&i| self.used[i])
            .map(|i| self.stops[i])
            .collect();
        let g = Gradient::new(live);
        self.channels = g.to_channels();
    }
}

impl ProcessNode for GradientSourceProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Gradient Source" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn process(&mut self) {
        // Stops change via load_data/pending_config, not per-tick. Nothing
        // to do here; `channels` is kept in sync by recompute_channels.
    }

    fn read_output(&self, channel: usize) -> f32 {
        self.channels.get(channel).copied().unwrap_or(0.0)
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        let stops: Vec<serde_json::Value> = (0..MAX_STOPS)
            .map(|i| {
                if !self.used[i] {
                    return serde_json::json!({ "used": false });
                }
                let s = &self.stops[i];
                serde_json::json!({
                    "used": true,
                    "position": s.position,
                    "r": s.color.r,
                    "g": s.color.g,
                    "b": s.color.b,
                    "alpha": s.alpha,
                })
            })
            .collect();
        Some(serde_json::json!({ "stops": stops }))
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Some(arr) = data.get("stops").and_then(|v| v.as_array()) {
            for (i, entry) in arr.iter().take(MAX_STOPS).enumerate() {
                let used = entry.get("used").and_then(|v| v.as_bool()).unwrap_or(false);
                self.used[i] = used;
                if !used { continue; }
                let position = entry.get("position").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let r = entry.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let g = entry.get("g").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let b = entry.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let alpha = entry.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                self.stops[i] = GradientStop {
                    position: position.clamp(0.0, 1.0),
                    color: Rgb::new(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
                    alpha: alpha.clamp(0.0, 1.0),
                };
            }
            self.recompute_channels();
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let active: Vec<(f32, Rgb, f32)> = (0..MAX_STOPS)
            .filter(|&i| self.used[i])
            .map(|i| {
                let s = &self.stops[i];
                (s.position, s.color, s.alpha)
            })
            .collect();
        shared.display = Some(Box::new(GradientSourceDisplay { stops: active }));
    }
}
