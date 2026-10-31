//! Effect Stack: the effects and other stacks linked to one name, blended in
//! order. A stack with nothing to send to is a root and writes to its groups;
//! the others blend into their parent, so stacks form a tree.

use std::any::Any;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::color::BlendMode;
use crate::dmx_io::SharedObjectStore;
use crate::engine::types::*;

use super::hub::{StackEntry, blend_layer, clear};
use super::{EffectHub, Geometry, LayerBuffer, RenderGate, TargetSpec, layer_level};

fn one() -> f32 {
    1.0
}

/// How the stack treats one of its effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerConfig {
    /// The effect node.
    pub node_id: u64,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
}

impl LayerConfig {
    pub fn new(node_id: u64) -> Self {
        Self { node_id, blend: BlendMode::Override, opacity: 1.0, muted: false, solo: false }
    }
}

/// What the stack does with the things sent to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackMode {
    /// Blend them bottom to top, each with its own blend and opacity.
    #[default]
    Layers,
    /// Show one of them at a time, fading from the old to the new one when
    /// the `scene` input changes, by the progress on the `fade` input.
    Scenes,
}

/// Ticks to ignore `fade` after the scene changes. The node that makes the
/// fade restarts it a tick or two later, and until then it still reads the
/// end of the last one.
const FADE_SETTLE_TICKS: u32 = 10;

/// Frames a stack waits for children that have not drawn yet before it shows
/// what it has. Until then it publishes nothing, so a parent that is fading
/// to it keeps showing the old scene instead of a blank frame.
const READY_PATIENCE: u32 = 8;

/// Which scene is showing and where a fade between two stands.
#[derive(Debug, Clone, Default, PartialEq)]
struct SceneFade {
    /// The scene shown when no fade is running.
    settled: usize,
    /// The scene being faded from, and the one the `scene` input asks for.
    from: usize,
    target: usize,
    fading: bool,
    progress: f32,
    ignore: u32,
    started: bool,
}

impl SceneFade {
    /// One tick. `asked` is the scene the input names, `fade` the progress
    /// input, or None when nothing is wired to it (the change is a cut).
    fn step(&mut self, asked: usize, fade: Option<f32>) {
        if !self.started {
            *self = Self { settled: asked, from: asked, target: asked, started: true, ..Default::default() };
            return;
        }
        if asked != self.target {
            // Fading again before the last fade is over starts from whichever
            // end it was closer to.
            self.from = if self.fading { if self.progress >= 0.5 { self.target } else { self.from } } else { self.settled };
            self.target = asked;
            self.progress = 0.0;
            self.ignore = FADE_SETTLE_TICKS;
            self.fading = fade.is_some();
            if !self.fading {
                self.settled = asked;
            }
        }
        if self.fading {
            if self.ignore > 0 {
                self.ignore -= 1;
                self.progress = 0.0;
            } else {
                self.progress = fade.unwrap_or(1.0).clamp(0.0, 1.0);
                if self.progress >= 0.999 {
                    self.settled = self.target;
                    self.fading = false;
                }
            }
        }
    }

    /// How much of each of `n` scenes shows, summing to 1.
    fn weights(&self, n: usize) -> Vec<f32> {
        let mut w = vec![0.0; n];
        if n == 0 {
            return w;
        }
        let at = |i: usize| i.min(n - 1);
        if self.fading {
            w[at(self.from)] += 1.0 - self.progress;
            w[at(self.target)] += self.progress;
        } else {
            w[at(self.settled)] = 1.0;
        }
        w
    }
}

/// Everything the UI sets on a stack.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackConfig {
    /// The name effects link to.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub group_ids: Vec<u32>,
    #[serde(default)]
    pub group_names: Vec<String>,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    /// The stack this one is sent to, by name. Empty for a root, which is the
    /// only kind that writes to the groups (the others take their geometry
    /// from their parent).
    #[serde(default)]
    pub parent: String,
    /// How a root blends over other roots on the same objects.
    #[serde(default)]
    pub blend: BlendMode,
    /// The effects and stacks sent here, top first, as the list reads on screen.
    #[serde(default)]
    pub layers: Vec<LayerConfig>,
    /// Show only the status, not the layer list.
    #[serde(default)]
    pub folded: bool,
    #[serde(default)]
    pub mode: StackMode,
}

/// One target in the preview: where it sits and what it shows.
#[derive(Clone)]
pub struct PreviewStrip {
    pub row: usize,
    /// Where the first and last pixel are on the group's 0..1 axis; the
    /// first is the larger for a reversed strip.
    pub start: f32,
    pub end: f32,
    pub colors: Vec<[u8; 3]>,
}

/// What the widget shows.
pub struct StackDisplay {
    pub pixels: usize,
    /// The composed image, at most 64 columns per target.
    pub preview: Vec<PreviewStrip>,
    /// Rows of the group's layout.
    pub rows: usize,
    /// Activity of each layer by effect node id.
    pub levels: Vec<(u64, f32)>,
    /// In scenes mode, how much of each scene shows (by node id) and the
    /// index of the scene asked for.
    pub scenes: Option<(Vec<(u64, f32)>, usize)>,
}

pub struct EffectStackProcessNode {
    id: NodeId,
    hub: EffectHub,
    store: SharedObjectStore,
    config: StackConfig,
    inputs: Vec<PortDef>,
    amount: f32,
    amount_wired: bool,
    scene_in: f32,
    fade_in: f32,
    fade_wired: bool,
    scene: SceneFade,
    /// What each scene counts for now, by node id, for the display.
    scene_weights: Vec<(u64, f32)>,
    /// The scene that was last on show, which keeps showing while the one
    /// asked for has not drawn its first frame yet.
    last_shown: Option<u64>,
    /// Frames in a row spent waiting for a child's first image.
    waiting: u32,
    /// The nodes this stack last judged as wanted or not, so the ones that
    /// left its list can be forgotten.
    judged: Vec<u64>,
    gate: RenderGate,
    geometry: Option<Arc<Geometry>>,
    composed: LayerBuffer,
    levels: Vec<(u64, f32)>,
    /// Name under which this stack's geometry is currently published.
    published_name: String,
}

impl EffectStackProcessNode {
    pub fn new(id: NodeId, hub: EffectHub, store: SharedObjectStore) -> Self {
        Self {
            id,
            hub,
            store,
            config: StackConfig::default(),
            inputs: vec![
                PortDef::new("amount", PortType::Untyped),
                PortDef::new("scene", PortType::Untyped),
                PortDef::new("fade", PortType::Phase),
            ],
            amount: 1.0,
            amount_wired: false,
            scene_in: 0.0,
            fade_in: 0.0,
            fade_wired: false,
            scene: SceneFade::default(),
            scene_weights: Vec::new(),
            last_shown: None,
            waiting: 0,
            judged: Vec::new(),
            gate: RenderGate::new(),
            geometry: None,
            composed: Vec::new(),
            levels: Vec::new(),
            published_name: String::new(),
        }
    }

    /// What a child of this stack publishes: its image, how bright it is, and
    /// how much it is scaled by its own `amount` (1 for an effect).
    fn child_image<'a>(
        hub: &'a super::hub::HubState,
        name: &str,
        node_id: u64,
    ) -> Option<(&'a LayerBuffer, f32, f32)> {
        if let Some(e) = hub.layers.get(&node_id) {
            return (e.stack == name).then_some((&e.buf, e.level, 1.0));
        }
        let e = hub.stacks.get(&node_id)?;
        (e.parent == name).then(|| (&e.buf, layer_level(&e.buf), e.amount))
    }

    /// Tell the children whether anything shows them, so the ones that do not
    /// are not rendered. A scene being faded to, or still showing until the
    /// new one has drawn, counts as shown.
    fn judge_children(&mut self, stack_wanted: bool) {
        let soloed = self.config.layers.iter().any(|l| l.solo);
        let weights = match self.config.mode {
            StackMode::Scenes => Some(self.scene.weights(self.config.layers.len())),
            StackMode::Layers => None,
        };
        let mut hub = self.hub.lock();
        for (i, layer) in self.config.layers.iter().enumerate() {
            let amount = hub.stacks.get(&layer.node_id).map_or(1.0, |e| e.amount);
            let shows = match &weights {
                Some(w) => {
                    w[i] > 0.0 || i == self.scene.target || self.last_shown == Some(layer.node_id)
                }
                None => !(soloed && !layer.solo),
            };
            let wanted = stack_wanted && shows && !layer.muted && layer.opacity > 0.0 && amount > 0.0;
            if wanted {
                hub.unwanted.remove(&layer.node_id);
            } else {
                hub.unwanted.insert(layer.node_id);
            }
        }
        // Whatever left the list is nobody's concern any more.
        for id in self.judged.iter().filter(|id| !self.config.layers.iter().any(|l| l.node_id == **id)) {
            hub.unwanted.remove(id);
        }
        self.judged = self.config.layers.iter().map(|l| l.node_id).collect();
    }

    /// Blend the children bottom to top, each as its list entry says. True
    /// when a child that should show has not drawn its first image yet.
    fn compose_layers(&mut self, geometry: &Geometry) -> bool {
        let soloed = self.config.layers.iter().any(|l| l.solo);
        let hub = self.hub.lock();
        let mut pending = false;
        // The list reads top first, so draw it bottom first.
        for layer in self.config.layers.iter().rev() {
            let shows = !layer.muted && !(soloed && !layer.solo) && layer.opacity > 0.0;
            let Some((buf, level, amount)) = Self::child_image(&hub, &self.config.name, layer.node_id) else {
                // Not published at all: an effect that has not started, but
                // not one that belongs to another stack.
                pending |= shows && !hub.layers.contains_key(&layer.node_id) && !hub.stacks.contains_key(&layer.node_id);
                continue;
            };
            self.levels.push((layer.node_id, level));
            if !shows {
                continue;
            }
            if geometry.fits(buf) {
                blend_layer(&mut self.composed, buf, layer.blend, layer.opacity * amount);
            } else {
                pending |= amount > 0.0;
            }
        }
        pending
    }

    /// Show the scenes by their weights. Blending them bottom to top with
    /// amount `w_k / (w_1 + ... + w_k)` makes the result exactly
    /// `sum(w_k * scene_k) / sum(w_k)`, so a fade between two scenes is a
    /// straight crossfade with no dip in the middle. The sum of the weights
    /// (a scene's own opacity and amount count) dims the whole.
    ///
    /// A scene that has not drawn yet (it was not wanted until it was asked
    /// for) cannot be shown, so its share goes to the scene that was showing
    /// last, and the change happens once it is ready. True when there is
    /// nothing to show in its place either.
    fn compose_scenes(&mut self, geometry: &Geometry) -> bool {
        let mut weights = self.scene.weights(self.config.layers.len());
        let hub = self.hub.lock();
        // Per scene, top of the list first: what it shows, if it can.
        let mut images: Vec<Option<(&LayerBuffer, f32)>> = Vec::new();
        for layer in &self.config.layers {
            let child = Self::child_image(&hub, &self.config.name, layer.node_id);
            if let Some((_, level, _)) = child {
                self.levels.push((layer.node_id, level));
            }
            images.push(child.filter(|(buf, _, _)| geometry.fits(buf)).map(|(buf, _, amount)| (buf, amount)));
        }

        let mut spill = 0.0;
        let mut unresolved = false;
        for ((layer, w), image) in self.config.layers.iter().zip(weights.iter_mut()).zip(&images) {
            if *w > 0.0 && image.is_none() && !layer.muted {
                spill += *w;
                *w = 0.0;
            }
        }
        if spill > 0.0 {
            let shown = |i: usize| images[i].is_some() && !self.config.layers[i].muted;
            let keeper = self
                .last_shown
                .and_then(|id| self.config.layers.iter().position(|l| l.node_id == id))
                .filter(|i| shown(*i))
                .or_else(|| (0..images.len()).filter(|i| shown(*i)).max_by(|a, b| weights[*a].total_cmp(&weights[*b])));
            match keeper {
                Some(i) => weights[i] += spill,
                None => unresolved = true,
            }
        }

        let mut scenes: Vec<(f32, Option<&LayerBuffer>)> = Vec::new();
        for ((layer, w), image) in self.config.layers.iter().zip(&weights).zip(&images) {
            let weight = match image {
                Some((_, amount)) if !layer.muted => w * layer.opacity * amount,
                _ => 0.0,
            };
            self.scene_weights.push((layer.node_id, weight));
            scenes.push((weight, image.map(|(buf, _)| buf)));
        }
        if let Some(i) = (0..scenes.len()).filter(|i| scenes[*i].0 > 0.0).max_by(|a, b| scenes[*a].0.total_cmp(&scenes[*b].0)) {
            self.last_shown = Some(self.config.layers[i].node_id);
        }

        let total: f32 = scenes.iter().map(|(w, _)| w).sum();
        if total <= 0.0 {
            return unresolved;
        }
        let mut cumulative = 0.0;
        for (weight, buf) in scenes.iter().rev() {
            cumulative += weight;
            if let (Some(buf), true) = (buf, *weight > 0.0) {
                blend_layer(&mut self.composed, buf, BlendMode::Override, weight / cumulative);
            }
        }
        let dim = total.min(1.0);
        if dim < 1.0 {
            for px in self.composed.iter_mut().flatten() {
                px.alpha *= dim;
            }
        }
        unresolved
    }

    fn unpublish(&mut self) {
        let mut hub = self.hub.lock();
        hub.stacks.remove(&self.id.0);
        for id in self.judged.drain(..) {
            hub.unwanted.remove(&id);
        }
        if !self.published_name.is_empty() {
            hub.geometry.remove(&self.published_name);
        }
        self.published_name.clear();
        self.geometry = None;
    }
}

impl Drop for EffectStackProcessNode {
    fn drop(&mut self) {
        self.unpublish();
    }
}

impl ProcessNode for EffectStackProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Effect Stack" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        match channel {
            0 => self.amount = value.clamp(0.0, 1.0),
            1 => self.scene_in = value,
            2 => self.fade_in = value,
            _ => {}
        }
    }

    fn read_input(&self, channel: usize) -> f32 {
        match channel {
            0 => self.amount,
            1 => self.scene_in,
            2 => self.fade_in,
            _ => 0.0,
        }
    }

    fn set_input_connections(&mut self, connected: &[bool]) {
        self.amount_wired = connected.first().copied().unwrap_or(false);
        self.fade_wired = connected.get(2).copied().unwrap_or(false);
    }

    fn process(&mut self) {
        // The scene follows its inputs every tick, not just when rendering.
        if self.config.mode == StackMode::Scenes {
            let last = self.config.layers.len().saturating_sub(1);
            let asked = (self.scene_in.round().max(0.0) as usize).min(last);
            self.scene.step(asked, self.fade_wired.then_some(self.fade_in));
        }
        if !self.gate.due() {
            return;
        }
        if self.config.name.is_empty() {
            self.unpublish();
            return;
        }
        let is_root = self.config.parent.is_empty();

        // A root works out its geometry from its groups, and pixel counts can
        // change when a fixture is edited, so look them up each frame. The
        // others inherit their parent's.
        let geometry = if is_root {
            if self.config.targets.is_empty() {
                self.unpublish();
                return;
            }
            let resolved = Geometry::resolve(&self.config.targets, &self.store.lock().unwrap());
            match &self.geometry {
                Some(g) if **g == resolved => g.clone(),
                _ => Arc::new(resolved),
            }
        } else {
            let parent = self.hub.lock().geometry.get(&self.config.parent).cloned();
            match parent {
                Some(g) => g,
                None => {
                    self.unpublish();
                    return;
                }
            }
        };
        let same = self.geometry.as_ref().is_some_and(|g| Arc::ptr_eq(g, &geometry));
        if !same || self.published_name != self.config.name {
            let mut hub = self.hub.lock();
            if !self.published_name.is_empty() && self.published_name != self.config.name {
                hub.geometry.remove(&self.published_name);
            }
            hub.geometry.insert(self.config.name.clone(), geometry.clone());
            drop(hub);
            self.published_name.clone_from(&self.config.name);
            self.geometry = Some(geometry.clone());
        }

        let amount = if self.amount_wired { self.amount } else { 1.0 };
        // A root is shown unless its amount is zero; the others when their
        // parent says so.
        let wanted = if is_root { amount > 0.0 } else { !self.hub.lock().unwanted.contains(&self.id.0) };
        self.judge_children(wanted);

        self.levels.clear();
        self.scene_weights.clear();
        if wanted {
            if !geometry.fits(&self.composed) {
                self.composed = geometry.empty_buffer();
            } else {
                clear(&mut self.composed);
            }
            let pending = match self.config.mode {
                StackMode::Layers => self.compose_layers(&geometry),
                StackMode::Scenes => self.compose_scenes(&geometry),
            };
            self.waiting = if pending { self.waiting + 1 } else { 0 };
            if pending && self.waiting <= READY_PATIENCE {
                self.composed = Vec::new();
            }
        } else {
            self.waiting = 0;
            // Not shown: no image, but the parent still reads the amount.
            self.composed = Vec::new();
        }

        let mut hub = self.hub.lock();
        match hub.stacks.get_mut(&self.id.0) {
            Some(entry) => {
                entry.name.clone_from(&self.config.name);
                entry.parent.clone_from(&self.config.parent);
                entry.blend = self.config.blend;
                entry.amount = amount;
                entry.geometry = geometry;
                entry.buf.clone_from(&self.composed);
            }
            None => {
                hub.stacks.insert(
                    self.id.0,
                    StackEntry {
                        name: self.config.name.clone(),
                        parent: self.config.parent.clone(),
                        blend: self.config.blend,
                        amount,
                        geometry,
                        buf: self.composed.clone(),
                    },
                );
            }
        }
    }

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(config) = serde_json::from_value::<StackConfig>(data.clone()) {
            self.config = config;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        // Shrink long strips to a thumbnail the widget can draw cheaply.
        const COLUMNS: usize = 64;
        let geometry = self.geometry.as_deref();
        let preview = self
            .composed
            .iter()
            .enumerate()
            .filter(|(_, strip)| !strip.is_empty())
            .map(|(t, strip)| {
                let n = strip.len().min(COLUMNS);
                let colors = (0..n)
                    .map(|c| {
                        let p = strip[c * strip.len() / n];
                        let (r, g, b) = p.color.scaled(p.alpha.clamp(0.0, 1.0)).clamped().to_u8();
                        [r, g, b]
                    })
                    .collect();
                let (row, start, end) = match geometry.and_then(|g| g.targets.get(t).map(|x| (g, x))) {
                    Some((g, target)) => {
                        let (start, end) = match &target.spec {
                            TargetSpec::Strip { logical_start, logical_end, .. } => (*logical_start, *logical_end),
                            TargetSpec::Fixture { position, .. } => (*position, *position),
                        };
                        (g.rows.get(t).copied().unwrap_or(0), start, end)
                    }
                    None => (t, 0.0, 1.0),
                };
                PreviewStrip { row, start, end, colors }
            })
            .collect();
        shared.display = Some(Box::new(StackDisplay {
            pixels: self.composed.iter().map(Vec::len).sum(),
            preview,
            rows: geometry.map_or(1, |g| g.row_count()),
            levels: self.levels.clone(),
            scenes: (self.config.mode == StackMode::Scenes)
                .then(|| (self.scene_weights.clone(), self.scene.target)),
        }));
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

#[cfg(test)]
mod scene_tests {
    use super::*;

    fn settled(f: &mut SceneFade, asked: usize, fade: Option<f32>) {
        for _ in 0..(FADE_SETTLE_TICKS + 2) {
            f.step(asked, fade);
        }
    }

    #[test]
    fn the_first_scene_asked_for_shows_at_once() {
        let mut f = SceneFade::default();
        f.step(2, Some(0.0));
        assert_eq!(f.weights(3), vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_change_without_a_fade_input_is_a_cut() {
        let mut f = SceneFade::default();
        f.step(0, None);
        f.step(1, None);
        assert_eq!(f.weights(2), vec![0.0, 1.0]);
    }

    #[test]
    fn a_change_fades_by_the_progress_once_the_old_end_of_it_has_been_ignored() {
        let mut f = SceneFade::default();
        f.step(0, Some(1.0));
        // The scene changes while the fade input still reads the end of the last fade.
        f.step(1, Some(1.0));
        assert_eq!(f.weights(2), vec![1.0, 0.0], "still the old scene");
        settled(&mut f, 1, Some(0.25));
        assert_eq!(f.weights(2), vec![0.75, 0.25]);
        f.step(1, Some(0.6));
        let w = f.weights(2);
        assert!((w[0] - 0.4).abs() < 1e-6 && (w[1] - 0.6).abs() < 1e-6, "{w:?}");
        f.step(1, Some(1.0));
        assert_eq!(f.weights(2), vec![0.0, 1.0], "done, and it stays");
        f.step(1, Some(0.0));
        assert_eq!(f.weights(2), vec![0.0, 1.0], "the fade input falling back changes nothing");
    }

    #[test]
    fn jumping_over_scenes_fades_straight_to_the_new_one() {
        let mut f = SceneFade::default();
        f.step(0, Some(1.0));
        f.step(3, Some(0.0));
        settled(&mut f, 3, Some(0.5));
        assert_eq!(f.weights(4), vec![0.5, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn asking_again_halfway_fades_from_the_end_it_was_nearer_to() {
        let mut f = SceneFade::default();
        f.step(0, Some(1.0));
        f.step(1, Some(0.0));
        settled(&mut f, 1, Some(0.8));
        f.step(2, Some(0.8));
        settled(&mut f, 2, Some(0.0));
        assert_eq!(f.weights(3), vec![0.0, 1.0, 0.0], "starts from scene 1, which was nearly there");
    }

    #[test]
    fn a_scene_past_the_end_of_the_list_is_the_last_one() {
        let mut f = SceneFade::default();
        f.step(5, None);
        assert_eq!(f.weights(3), vec![0.0, 0.0, 1.0]);
        assert_eq!(f.weights(0), Vec::<f32>::new());
    }
}
