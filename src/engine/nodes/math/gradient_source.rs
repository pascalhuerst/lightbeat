//! Gradient Source — emits an 8-stop gradient on a single `Gradient` output.
//! The stops are authored in the widget inspector (color picker + position
//! slider + alpha slider per stop) and packed into the 40-channel flat
//! representation `Gradient::to_channels` expects.

use serde::{Deserialize, Serialize};

use crate::color::{Gradient, GradientStop, Rgb};
use crate::engine::types::*;

const MAX_STOPS: usize = GRADIENT_STOP_COUNT; // 8

/// The stops as saved and as the widget pushes them through `pending_config`.
/// A slot that is not used is saved as `{"used": false}` alone.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GradientConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stops: Option<Vec<StopConfig>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StopConfig {
    pub used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    g: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    b: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alpha: Option<f32>,
}

impl StopConfig {
    pub fn unused() -> Self {
        Self::default()
    }

    pub fn full(used: bool, position: f32, r: f32, g: f32, b: f32, alpha: f32) -> Self {
        Self { used, position: Some(position), r: Some(r), g: Some(g), b: Some(b), alpha: Some(alpha) }
    }

    /// `(position, r, g, b, alpha)`, with the defaults a missing key has always had.
    pub fn values(&self) -> (f32, f32, f32, f32, f32) {
        (
            self.position.unwrap_or(0.0),
            self.r.unwrap_or(0.0),
            self.g.unwrap_or(0.0),
            self.b.unwrap_or(0.0),
            self.alpha.unwrap_or(1.0),
        )
    }
}

impl GradientConfig {
    pub fn new(stops: Vec<StopConfig>) -> Self {
        Self { stops: Some(stops) }
    }

    /// Malformed data loads as an empty update, which changes nothing.
    pub fn from_value(v: &serde_json::Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }

    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

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

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports() -> (Vec<PortDef>, Vec<PortDef>) {
    (vec![], vec![PortDef::new("gradient", PortType::Gradient)])
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
            outputs: ports().1,
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
        let stops = (0..MAX_STOPS)
            .map(|i| {
                if !self.used[i] {
                    return StopConfig::unused();
                }
                let s = &self.stops[i];
                StopConfig::full(true, s.position, s.color.r, s.color.g, s.color.b, s.alpha)
            })
            .collect();
        Some(GradientConfig::new(stops).to_value())
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        let Some(stops) = GradientConfig::from_value(data).stops else { return };
        for (i, entry) in stops.iter().take(MAX_STOPS).enumerate() {
            self.used[i] = entry.used;
            if !entry.used { continue; }
            let (position, r, g, b, alpha) = entry.values();
            self.stops[i] = GradientStop {
                position: position.clamp(0.0, 1.0),
                color: Rgb::new(r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)),
                alpha: alpha.clamp(0.0, 1.0),
            };
        }
        self.recompute_channels();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_written_save_shape_loads_and_saves_unchanged() {
        let mut stops = vec![
            serde_json::json!({"used": true, "position": 0.25, "r": 1.0, "g": 0.5, "b": 0.0, "alpha": 0.75}),
            serde_json::json!({"used": true, "position": 1.0, "r": 0.0, "g": 0.0, "b": 1.0, "alpha": 1.0}),
        ];
        stops.resize(MAX_STOPS, serde_json::json!({"used": false}));
        let saved = serde_json::json!({ "stops": stops });
        let mut n = GradientSourceProcessNode::new(NodeId(1));
        n.load_data(&saved);
        assert_eq!(n.stops[0].position, 0.25);
        assert_eq!(n.stops[0].color.g, 0.5);
        assert_eq!(n.stops[0].alpha, 0.75);
        assert!(!n.used[2]);
        assert_eq!(n.save_data(), Some(saved));
    }

    #[test]
    fn a_used_stop_with_missing_keys_gets_the_old_defaults() {
        let mut n = GradientSourceProcessNode::new(NodeId(1));
        n.load_data(&serde_json::json!({"stops": [{"used": true}]}));
        assert_eq!((n.stops[0].position, n.stops[0].alpha), (0.0, 1.0));
    }
}
