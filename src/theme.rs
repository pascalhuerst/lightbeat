//! Central theme constants for the UI. All semantically meaningful colours
//! live here so the whole app's palette can be re-skinned from a single
//! place. Widget-local, one-off decorative greys generally stay in-file.
//!
//! Naming convention:
//! - `NODE_*`   — node chrome (body, title, border, shadows, …)
//! - `PORT_*`   — port-type colours (used by `PortTypeUi::color()`)
//! - `WIRE_*`   — anything connection-related (selected, hovered, …)
//! - `FRAME_*`  — decorative-frame defaults
//! - `CANVAS_*` — canvas background / grid
//! - `STATUS_*` — semantic state (warning, danger, success-ish)
//! - `METER_*`  — peak-meter band colours
//! - `TEXT_*`   — body-text colours across the various emphasis levels
//! - `UI_*`     — miscellaneous control chrome (dashed selection, override
//!   overlays, etc.)

use egui::Color32;

// ---------------------------------------------------------------------------
// Node chrome
// ---------------------------------------------------------------------------

pub const NODE_BG: Color32 = Color32::from_rgb(38, 38, 42);
pub const NODE_BG_DISABLED: Color32 = Color32::from_gray(32);
pub const NODE_TITLE_BG: Color32 = Color32::from_rgb(50, 50, 56);
pub const NODE_TITLE_BG_DISABLED: Color32 = Color32::from_gray(38);
pub const NODE_BORDER: Color32 = Color32::from_rgb(70, 70, 78);
pub const NODE_BORDER_DISABLED: Color32 = Color32::from_gray(80);
pub const NODE_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 60);
// Premultiplied form of unmultiplied (28, 28, 30, 225) — near-opaque
// dark-grey wash used over disabled node content.
pub const NODE_TITLE_DISABLED_OVERLAY: Color32 =
    Color32::from_rgba_premultiplied(25, 25, 26, 225);

// ---------------------------------------------------------------------------
// Selection / peer
// ---------------------------------------------------------------------------

pub const SELECTED_BORDER: Color32 = Color32::from_rgb(100, 160, 255);
pub const PORTAL_PEER_BORDER: Color32 = Color32::from_rgb(220, 180, 80);
// Premultiplied form of unmultiplied (220, 180, 80, 40).
pub const PORTAL_PEER_HALO: Color32 = Color32::from_rgba_premultiplied(34, 28, 13, 40);

// ---------------------------------------------------------------------------
// Canvas
// ---------------------------------------------------------------------------

pub const CANVAS_BG: Color32 = Color32::from_rgb(22, 22, 26);
pub const GRID: Color32 = Color32::from_rgb(30, 30, 34);

// ---------------------------------------------------------------------------
// Port-type colours (consumed by `PortTypeUi::color()`)
// ---------------------------------------------------------------------------

pub const PORT_LOGIC: Color32 = Color32::from_rgb(240, 200, 40);
pub const PORT_PHASE: Color32 = Color32::from_rgb(180, 100, 220);
pub const PORT_UNTYPED: Color32 = Color32::from_rgb(220, 80, 80);
pub const PORT_ANY: Color32 = Color32::from_gray(160);
pub const PORT_COLOR: Color32 = Color32::from_rgb(60, 200, 220);
pub const PORT_POSITION: Color32 = Color32::from_rgb(80, 200, 140);
pub const PORT_PALETTE: Color32 = Color32::from_rgb(220, 180, 100);
pub const PORT_GRADIENT: Color32 = Color32::from_rgb(40, 200, 180);

// ---------------------------------------------------------------------------
// Port body
// ---------------------------------------------------------------------------

pub const PORT_DISABLED_FILL: Color32 = Color32::from_gray(40);
pub const PORT_DISABLED_STROKE: Color32 = Color32::from_gray(70);

// ---------------------------------------------------------------------------
// Wires
// ---------------------------------------------------------------------------

// Premultiplied form of unmultiplied (255, 255, 255, 180).
pub const WIRE_SELECTED_HALO: Color32 = Color32::from_rgba_premultiplied(180, 180, 180, 180);

// ---------------------------------------------------------------------------
// Selection rect
// ---------------------------------------------------------------------------

pub const SELECTION_RECT_FILL: Color32 = Color32::from_rgba_premultiplied(100, 160, 255, 30);

// ---------------------------------------------------------------------------
// Node accents
// ---------------------------------------------------------------------------

/// Rose/magenta "this is a container" accent for Subgraph nodes.
pub const ACCENT_SUBGRAPH: Color32 = Color32::from_rgb(190, 90, 150);
/// Amber accent used by Portal In / Portal Out nodes (matches the peer halo).
pub const ACCENT_PORTAL: Color32 = Color32::from_rgb(220, 170, 60);

// ---------------------------------------------------------------------------
// State / warning / danger
// ---------------------------------------------------------------------------

/// Amber used for "overridden by something" indicators (fader overrides,
/// override-on-wire hints, etc.).
pub const STATUS_OVERRIDE: Color32 = Color32::from_rgb(220, 150, 40);
pub const STATUS_OVERRIDE_ICON: Color32 = Color32::from_rgb(255, 180, 60);
pub const STATUS_OVERRIDE_OVERLAY: Color32 = Color32::from_rgba_premultiplied(220, 150, 40, 130);

/// Subtle positive indicator (meter green, successes, "live" dots).
pub const STATUS_OK: Color32 = Color32::from_rgb(80, 200, 140);

/// Cyan used for UI toggles / fader fills / XY pad knob — neutral active.
pub const STATUS_ACTIVE: Color32 = Color32::from_rgb(80, 200, 240);

/// Amber used for non-critical warnings in body text ("duplicate name",
/// "not connected"). Slightly brighter/redder than `STATUS_OVERRIDE` so it
/// reads as a message rather than a control-override chrome element.
pub const STATUS_WARNING: Color32 = Color32::from_rgb(220, 150, 60);

// ---------------------------------------------------------------------------
// Peak meter
// ---------------------------------------------------------------------------

pub const METER_GREEN: Color32 = Color32::from_rgb(80, 200, 100);
pub const METER_YELLOW: Color32 = Color32::from_rgb(230, 200, 60);
pub const METER_RED: Color32 = Color32::from_rgb(230, 70, 60);
pub const METER_PEAK_HOLD: Color32 = Color32::from_rgb(220, 220, 220);
pub const METER_RMS_OVERLAY: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 128);

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

pub const TEXT_PRIMARY: Color32 = Color32::WHITE;
pub const TEXT_DISABLED: Color32 = Color32::from_gray(150);
pub const TEXT_SUBTLE: Color32 = Color32::from_gray(200);
pub const TEXT_MUTED: Color32 = Color32::from_gray(180);
pub const TEXT_DIM: Color32 = Color32::from_gray(140);
pub const TEXT_FAINT: Color32 = Color32::from_gray(120);

// ---------------------------------------------------------------------------
// Generic dark backgrounds (tooltips, scope bg, checkerboard etc.)
// ---------------------------------------------------------------------------

pub const SCOPE_BG: Color32 = Color32::from_gray(24);
pub const SCOPE_BORDER: Color32 = Color32::from_gray(80);
pub const SCOPE_ZERO_LINE: Color32 = Color32::from_gray(60);

pub const CHECKER_A: Color32 = Color32::from_gray(40);
pub const CHECKER_B: Color32 = Color32::from_gray(70);
