//! Named sets of a node's settings that can be recalled and faded between.
//!
//! A snapshot stores every setting the node lets the user promote, by name, so
//! it survives a node reordering its params. The list belongs to the UI (it is
//! saved with the node and edited in the inspector); the engine gets a copy and
//! runs the recall and the fade, driven by the select wire, the fade wire or a
//! click in the list.

use serde::{Deserialize, Serialize};

use super::types::{ParamDef, ParamValue, ProcessNode};

/// Ports the graph adds for snapshots are promoted entries with these names.
pub const SELECT_NAME: &str = "Snapshot select";
pub const FADE_NAME: &str = "Snapshot fade";

/// Ticks a fade phase is ignored after a recall: the node that makes the phase
/// restarts a tick or two later, and until then it still reads the old end.
const SETTLE_TICKS: u32 = 4;
const DT: f32 = 0.001;
const EPS: f32 = 1e-4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SnapValue {
    Float(f32),
    Int(i64),
    Bool(bool),
    Choice(usize),
}

impl SnapValue {
    fn of(def: &ParamDef) -> Self {
        match def {
            ParamDef::Float { value, .. } => SnapValue::Float(*value),
            ParamDef::Int { value, .. } => SnapValue::Int(*value),
            ParamDef::Bool { value, .. } => SnapValue::Bool(*value),
            ParamDef::Choice { value, .. } => SnapValue::Choice(*value),
        }
    }

    fn to_param(&self) -> ParamValue {
        match self {
            SnapValue::Float(v) => ParamValue::Float(*v),
            SnapValue::Int(v) => ParamValue::Int(*v),
            SnapValue::Bool(v) => ParamValue::Bool(*v),
            SnapValue::Choice(v) => ParamValue::Choice(*v),
        }
    }

    /// Floats and integers blend; a dropdown or a checkbox has nothing in
    /// between, so it switches halfway.
    pub fn blend(&self, to: &SnapValue, t: f32) -> SnapValue {
        match (self, to) {
            (SnapValue::Float(a), SnapValue::Float(b)) => SnapValue::Float(a + (b - a) * t),
            (SnapValue::Int(a), SnapValue::Int(b)) => {
                SnapValue::Int((*a as f32 + (*b - *a) as f32 * t).round() as i64)
            }
            _ => if t >= 0.5 { to.clone() } else { self.clone() },
        }
    }

    fn same(&self, other: &SnapValue) -> bool {
        match (self, other) {
            (SnapValue::Float(a), SnapValue::Float(b)) => (a - b).abs() <= EPS * a.abs().max(1.0),
            _ => self == other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub name: String,
    pub values: Vec<(String, SnapValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotSet {
    pub snapshots: Vec<Snapshot>,
    /// Seconds a change takes when no fade wire is connected; 0 is a cut.
    pub fade_time: f32,
    /// The select input is a 0..1 signal spread over the list instead of an
    /// index, so a fader or a phase can walk through it.
    pub spread: bool,
    /// The snapshot last recalled or stored. The engine is the authority; the
    /// UI mirrors it and writes it back when it saves.
    pub active: Option<u64>,
    pub next_id: u64,
}

impl Default for SnapshotSet {
    fn default() -> Self {
        Self { snapshots: Vec::new(), fade_time: 0.0, spread: false, active: None, next_id: 1 }
    }
}

impl SnapshotSet {
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    pub fn add(&mut self, name: Option<String>, values: Vec<(String, SnapValue)>) -> u64 {
        let id = self.next_id.max(self.snapshots.iter().map(|s| s.id + 1).max().unwrap_or(1));
        self.next_id = id + 1;
        let name = name.unwrap_or_else(|| format!("Snapshot {}", self.snapshots.len() + 1));
        self.snapshots.push(Snapshot { id, name, values });
        id
    }

    pub fn get(&self, id: u64) -> Option<&Snapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }

    /// True when the active snapshot no longer matches the settings: the user
    /// moved something after recalling it.
    pub fn modified(&self, params: &[ParamDef], skip: &[&str]) -> bool {
        let Some(snap) = self.active.and_then(|id| self.get(id)) else { return false };
        snap.values.iter().any(|(name, v)| {
            params.iter().find(|p| p.name() == name)
                .is_some_and(|p| !SnapValue::of(p).same(v))
        }) || capture(params, skip).iter().any(|(name, _)| !snap.values.iter().any(|(n, _)| n == name))
    }
}

/// A node's `non_promotable_params` are either structural (modes, counts that
/// reshape its ports, not for a fade to flip) or numbers that a native input
/// already serves. The numbers are worth storing; the rest is not.
pub fn storable(p: &ParamDef, skip: &[&str]) -> bool {
    !skip.contains(&p.name()) || matches!(p, ParamDef::Float { .. })
}

/// Every setting that can be stored.
pub fn capture(params: &[ParamDef], skip: &[&str]) -> Vec<(String, SnapValue)> {
    params.iter()
        .filter(|p| storable(p, skip))
        .map(|p| (p.name().to_string(), SnapValue::of(p)))
        .collect()
}

/// A recall in progress for one node. Lives next to the promoted ports.
#[derive(Default)]
pub struct Player {
    set: SnapshotSet,
    current: Option<u64>,
    /// (param index, value when the recall began, value to reach)
    plan: Vec<(usize, SnapValue, SnapValue)>,
    /// 0..1 along the fade of a timed recall; 1 means settled.
    progress: f32,
    settle: u32,
    /// What was last written, so a param is only set when it moves.
    last_pos: Option<f32>,
    pub fade_wired: bool,
    pub select_wired: bool,
    fade_phase: f32,
    last_index: Option<usize>,
}

impl Player {
    pub fn is_idle(&self) -> bool {
        self.set.is_empty()
    }

    pub fn active(&self) -> Option<u64> {
        self.current
    }

    pub fn fading(&self) -> bool {
        !self.plan.is_empty() && (self.progress < 1.0 || self.fade_wired && self.last_pos.is_some_and(|p| p < 1.0))
    }

    /// Take the UI's copy of the list. The UI's `active` wins: it only differs
    /// from ours when the UI itself changed it (a stored snapshot becomes the
    /// active one).
    pub fn adopt(&mut self, set: SnapshotSet) {
        if self.current != set.active {
            self.plan.clear();
            self.progress = 1.0;
        }
        self.current = set.active.filter(|id| set.get(*id).is_some());
        self.set = set;
        self.last_index = None;
    }

    pub fn set_fade_phase(&mut self, v: f32) {
        self.fade_phase = v;
    }

    /// The select wire's value for this tick.
    pub fn select_signal(&mut self, v: f32, node: &mut dyn ProcessNode) {
        let n = self.set.snapshots.len();
        if n == 0 {
            return;
        }
        let idx = if self.set.spread {
            ((v.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1)
        } else {
            (v.round().max(0.0) as usize).min(n - 1)
        };
        if self.last_index != Some(idx) {
            self.last_index = Some(idx);
            let id = self.set.snapshots[idx].id;
            // Seeing the index the node is already on (first tick after a
            // load) must not undo what the user changed since.
            if self.current != Some(id) {
                self.recall_id(id, node);
            }
        }
    }

    /// Recall by position in the list (a click), always re-applying it.
    pub fn recall(&mut self, index: usize, node: &mut dyn ProcessNode) {
        if let Some(id) = self.set.snapshots.get(index).map(|s| s.id) {
            self.last_index = Some(index);
            self.recall_id(id, node);
        }
    }

    fn recall_id(&mut self, id: u64, node: &mut dyn ProcessNode) {
        let Some(target) = self.set.get(id).cloned() else { return };
        let params = node.params();
        let skip = node.non_promotable_params();
        self.plan = target.values.iter()
            .filter_map(|(name, to)| {
                let i = params.iter().position(|p| p.name() == name)?;
                if !storable(&params[i], skip) {
                    return None;
                }
                Some((i, SnapValue::of(&params[i]), to.clone()))
            })
            .collect();
        self.current = Some(id);
        self.last_pos = None;
        self.progress = 0.0;
        self.settle = if self.fade_wired { SETTLE_TICKS } else { 0 };
        if !self.fade_wired && self.set.fade_time <= 0.0 {
            self.write(node, 1.0);
            self.progress = 1.0;
        }
    }

    fn write(&mut self, node: &mut dyn ProcessNode, pos: f32) {
        for (i, from, to) in &self.plan {
            let v = if pos >= 1.0 { to.clone() } else { from.blend(to, pos) };
            node.set_param(*i, v.to_param());
        }
        self.last_pos = Some(pos);
    }

    /// Advance a recall by one engine tick.
    pub fn step(&mut self, node: &mut dyn ProcessNode) {
        if self.plan.is_empty() {
            return;
        }
        if self.fade_wired {
            // The phase is the position: it can run on to 1 and back again, and
            // when it stands still the settings are left to the user.
            let pos = if self.settle > 0 { self.settle -= 1; 0.0 } else { self.fade_phase.clamp(0.0, 1.0) };
            if self.last_pos != Some(pos) {
                self.write(node, pos);
            }
            self.progress = pos;
        } else if self.progress < 1.0 {
            self.progress = (self.progress + DT / self.set.fade_time.max(DT)).min(1.0);
            self.write(node, self.progress);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::nodes::math::scale_offset::ScaleOffsetProcessNode;
    use crate::engine::types::NodeId;

    fn node() -> ScaleOffsetProcessNode {
        ScaleOffsetProcessNode::new(NodeId(1))
    }

    fn float(node: &dyn ProcessNode, name: &str) -> f32 {
        node.params().iter().find(|p| p.name() == name).map(|p| match p {
            ParamDef::Float { value, .. } => *value,
            _ => panic!("not a float"),
        }).unwrap()
    }

    fn set_two(n: &mut ScaleOffsetProcessNode, fade: f32) -> SnapshotSet {
        let mut set = SnapshotSet { fade_time: fade, ..Default::default() };
        let skip = n.non_promotable_params();
        n.set_param(0, ParamValue::Float(1.0));
        set.add(None, capture(&n.params(), skip));
        n.set_param(0, ParamValue::Float(3.0));
        set.add(Some("loud".into()), capture(&n.params(), skip));
        set
    }

    #[test]
    fn a_cut_applies_at_once() {
        let mut n = node();
        let set = set_two(&mut n, 0.0);
        let mut p = Player::default();
        p.adopt(set);
        p.recall(0, &mut n);
        assert_eq!(float(&n, "Scale"), 1.0);
        assert_eq!(p.active(), Some(1));
        assert!(!p.fading());
    }

    #[test]
    fn a_timed_fade_blends_and_lands() {
        let mut n = node();
        let set = set_two(&mut n, 0.1);
        let mut p = Player::default();
        p.adopt(set);
        p.recall(0, &mut n);
        for _ in 0..50 { p.step(&mut n); }
        let mid = float(&n, "Scale");
        assert!(mid > 1.0 && mid < 3.0, "{mid}");
        for _ in 0..100 { p.step(&mut n); }
        assert_eq!(float(&n, "Scale"), 1.0);
    }

    #[test]
    fn a_fade_phase_goes_there_and_back() {
        let mut n = node();
        let mut set = set_two(&mut n, 0.0);
        set.active = Some(2);
        let mut p = Player::default();
        p.adopt(set);
        p.fade_wired = true;
        p.recall(0, &mut n);
        for _ in 0..SETTLE_TICKS + 1 { p.step(&mut n); }
        p.set_fade_phase(0.5);
        p.step(&mut n);
        assert!((float(&n, "Scale") - 2.0).abs() < 1e-4);
        p.set_fade_phase(1.0);
        p.step(&mut n);
        assert_eq!(float(&n, "Scale"), 1.0);
        p.set_fade_phase(0.0);
        p.step(&mut n);
        assert_eq!(float(&n, "Scale"), 3.0);
    }

    #[test]
    fn editing_after_a_recall_marks_it_modified() {
        let mut n = node();
        let mut set = set_two(&mut n, 0.0);
        set.active = Some(1);
        n.set_param(0, ParamValue::Float(1.0));
        assert!(!set.modified(&n.params(), &[]));
        n.set_param(0, ParamValue::Float(1.5));
        assert!(set.modified(&n.params(), &[]));
    }

    #[test]
    fn the_select_wire_recalls_only_on_change() {
        let mut n = node();
        let set = set_two(&mut n, 0.0);
        let mut p = Player::default();
        p.adopt(set);
        p.select_signal(1.0, &mut n);
        assert_eq!(float(&n, "Scale"), 3.0);
        n.set_param(0, ParamValue::Float(2.0));
        p.select_signal(1.0, &mut n);
        assert_eq!(float(&n, "Scale"), 2.0);
        p.select_signal(0.0, &mut n);
        assert_eq!(float(&n, "Scale"), 1.0);
    }
}
