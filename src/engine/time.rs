//! The length of the tick being processed, for nodes that count time in steps.
//!
//! `process()` has no time argument and most nodes do not need one. The few
//! that integrate over time (envelope, time to phase, snapshot fades) read it
//! here. The engine loop sets it before each tick from the measured time since
//! the previous tick; it is per thread, so tests that call `process()` directly
//! see the nominal 1 ms unless they set another value.

use std::cell::Cell;

/// The step the loop aims for.
pub const NOMINAL_DT: f32 = 0.001;
const MIN_DT: f32 = 0.0001;
const MAX_DT: f32 = 0.010;

thread_local! {
    static DT: Cell<f32> = const { Cell::new(NOMINAL_DT) };
}

/// Seconds since the previous tick.
pub fn dt() -> f32 {
    DT.with(|d| d.get())
}

/// Clamped, so a stalled thread does not make an envelope jump and a bogus
/// zero does not freeze one.
pub fn set_dt(seconds: f32) {
    DT.with(|d| d.set(seconds.clamp(MIN_DT, MAX_DT)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dt_is_clamped_and_defaults_to_nominal() {
        assert_eq!(dt(), NOMINAL_DT);
        set_dt(5.0);
        assert_eq!(dt(), MAX_DT);
        set_dt(0.0);
        assert_eq!(dt(), MIN_DT);
        set_dt(NOMINAL_DT);
    }
}
