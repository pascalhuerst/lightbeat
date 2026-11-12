//! Lets the UI watch one root-level port at the full tick rate.
//!
//! The UI mirror of a node's ports is refreshed every 16th tick, which is fine
//! for a number on screen but loses a one-tick pulse or a fast wiggle, so the
//! hover scope showed nothing for signals a Scope node displays happily. While
//! a port is hovered the engine records its value every tick; the UI drains
//! the batch each frame.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::types::NodeId;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProbeTarget {
    pub node: NodeId,
    pub output: bool,
    pub channel: usize,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<(Option<ProbeTarget>, Vec<f32>)> = Mutex::new((None, Vec::new()));

/// Cap so a UI that stops draining cannot grow the batch without bound.
const MAX_BATCH: usize = 4096;

pub fn set_target(target: Option<ProbeTarget>) {
    let mut s = STATE.lock().unwrap();
    if s.0 != target {
        s.0 = target;
        s.1.clear();
    }
    ACTIVE.store(target.is_some(), Ordering::Relaxed);
}

pub fn target() -> Option<ProbeTarget> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    STATE.lock().unwrap().0
}

pub fn record(value: f32) {
    let mut s = STATE.lock().unwrap();
    if s.1.len() < MAX_BATCH {
        s.1.push(value);
    }
}

/// Everything recorded since the last call.
pub fn drain() -> Vec<f32> {
    std::mem::take(&mut STATE.lock().unwrap().1)
}

/// The sample of a batch that deserves the pixel: the one farthest from
/// `reference`, so a short pulse is not averaged or skipped away.
pub fn most_extreme(batch: &[f32], reference: f32) -> Option<f32> {
    batch.iter().copied().max_by(|a, b| {
        (a - reference).abs().partial_cmp(&(b - reference).abs()).unwrap_or(std::cmp::Ordering::Equal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_wins_over_rest() {
        assert_eq!(most_extreme(&[0.0, 0.0, 1.0, 0.0], 0.0), Some(1.0));
        assert_eq!(most_extreme(&[], 0.0), None);
    }
}
