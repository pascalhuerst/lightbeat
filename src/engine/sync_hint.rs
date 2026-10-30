//! Lets the UI tell the engine how fresh `NodeSharedState::save_data` needs to be.
//!
//! Building every node's `save_data` JSON was about three quarters of the
//! engine's UI mirror cost, yet it is only read by actions the user starts:
//! save, copy, moving nodes between levels, macros and undo snapshots. While
//! the user is idle it can lag; while they interact it must not.

use std::sync::atomic::{AtomicBool, Ordering};

use super::graph::UI_SYNC_EVERY_N_TICKS;

static INTERACTING: AtomicBool = AtomicBool::new(true);

/// Idle refresh interval, in UI syncs (so roughly 128 ticks).
const IDLE_EVERY_N_SYNCS: u32 = 8;

/// Called by the UI each frame. True while the user has recently interacted.
pub fn set_interacting(interacting: bool) {
    INTERACTING.store(interacting, Ordering::Relaxed);
}

/// Decides, per UI sync, whether `save_data` should be rebuilt.
#[derive(Default)]
pub struct SaveDataGate {
    syncs_since: u32,
}

impl SaveDataGate {
    pub fn due(&mut self) -> bool {
        self.syncs_since += 1;
        if INTERACTING.load(Ordering::Relaxed) || self.syncs_since >= IDLE_EVERY_N_SYNCS {
            self.syncs_since = 0;
            true
        } else {
            false
        }
    }
}

const _: () = assert!(UI_SYNC_EVERY_N_TICKS * IDLE_EVERY_N_SYNCS <= 256);
