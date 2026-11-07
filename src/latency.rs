//! The delay between the engine and the light in the room, one value for
//! every clock.
//!
//! The DMX path and the fixtures take a moment, so a clock that is exactly
//! on the beat makes lights that are late. Each clock source runs this far
//! ahead instead. It is a time, not a phase: the same milliseconds are a
//! larger part of a beat the faster the tempo.

use std::sync::atomic::{AtomicU32, Ordering};

/// How far ahead (or, negative, behind) the clocks can be run.
pub const LIMIT_MS: f32 = 500.0;

static LEAD_MS: AtomicU32 = AtomicU32::new(0);

pub fn lead_ms() -> f32 {
    f32::from_bits(LEAD_MS.load(Ordering::Relaxed))
}

pub fn set_lead_ms(ms: f32) {
    LEAD_MS.store(ms.clamp(-LIMIT_MS, LIMIT_MS).to_bits(), Ordering::Relaxed);
}

/// `ms` as a number of beats at `bpm`.
pub fn beats(ms: f32, bpm: f64) -> f64 {
    ms as f64 / 1000.0 * bpm / 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_delay_is_more_beats_at_a_faster_tempo() {
        assert!((beats(250.0, 120.0) - 0.5).abs() < 1e-9);
        assert!((beats(250.0, 240.0) - 1.0).abs() < 1e-9);
        assert!((beats(-250.0, 120.0) + 0.5).abs() < 1e-9);
    }
}
