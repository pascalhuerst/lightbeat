//! Shared types for the UI node family (Fader, Fader Group, Button, Button Group).

/// Combined configuration for whether mouse can override an input-driven
/// fader/button cell, and (if so) how the override gets cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseOverrideMode {
    /// Mouse cannot override; input fully drives the output.
    No,
    /// Mouse can override; double-click clears it.
    ClearOnReset,
    /// Mouse can override; cleared when the input value rises across the override value.
    PickupIncrease,
    /// Mouse can override; cleared when the input value falls across the override value.
    PickupDecrease,
}

impl Default for MouseOverrideMode {
    fn default() -> Self { MouseOverrideMode::No }
}

impl MouseOverrideMode {
    pub fn label(&self) -> &'static str {
        match self {
            MouseOverrideMode::No => "No",
            MouseOverrideMode::ClearOnReset => "Yes, clear on reset",
            MouseOverrideMode::PickupIncrease => "Yes, pickup increase",
            MouseOverrideMode::PickupDecrease => "Yes, pickup decrease",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "clear_on_reset" => MouseOverrideMode::ClearOnReset,
            "pickup_increase" => MouseOverrideMode::PickupIncrease,
            "pickup_decrease" => MouseOverrideMode::PickupDecrease,
            _ => MouseOverrideMode::No,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MouseOverrideMode::No => "no",
            MouseOverrideMode::ClearOnReset => "clear_on_reset",
            MouseOverrideMode::PickupIncrease => "pickup_increase",
            MouseOverrideMode::PickupDecrease => "pickup_decrease",
        }
    }

    pub fn allows_override(&self) -> bool {
        !matches!(self, MouseOverrideMode::No)
    }

    /// Evaluate whether an input-value transition should clear the override.
    pub fn should_clear(&self, prev_input: f32, cur_input: f32, override_value: f32) -> bool {
        match self {
            MouseOverrideMode::No | MouseOverrideMode::ClearOnReset => false,
            MouseOverrideMode::PickupIncrease => {
                prev_input <= override_value && cur_input > override_value
            }
            MouseOverrideMode::PickupDecrease => {
                prev_input >= override_value && cur_input < override_value
            }
        }
    }
}
