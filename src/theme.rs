//! Central theme constants for the UI.
//!
//! Three-tier palette:
//! - **Neutrals** (`TEXT_*`, `BG_*`, `STROKE`) — single grayscale ramp used for
//!   all chrome, backgrounds, and text.
//! - **Semantic states** (`SEM_*`) — four roles that describe *why* something
//!   has a colour: primary/selection, success/ok, warning, danger.
//! - **Data-type palette** (`TYPE_*`) — one colour per `PortType`. These are
//!   intentionally distinct hues so ports read at a glance.
//!
//! `ACCENT_SUBGRAPH` is the one special case — it's neither a semantic state
//! nor a data type, it just marks "this is a container".

use egui::Color32;

// ---------------------------------------------------------------------------
// Neutrals — grayscale ramp, named by role (bright → dark)
// ---------------------------------------------------------------------------

pub const TEXT_BRIGHT: Color32 = Color32::WHITE;
pub const TEXT: Color32 = Color32::from_gray(220);
pub const TEXT_MUTED: Color32 = Color32::from_gray(180);
pub const TEXT_DIM: Color32 = Color32::from_gray(140);
pub const STROKE: Color32 = Color32::from_gray(80);
pub const BG_HIGH: Color32 = Color32::from_gray(55);
pub const BG: Color32 = Color32::from_gray(40);
pub const BG_GRID: Color32 = Color32::from_gray(30);
pub const BG_DEEP: Color32 = Color32::from_gray(22);

// ---------------------------------------------------------------------------
// Semantic states
// ---------------------------------------------------------------------------

/// Blue — selection, active UI, interactive highlight.
pub const SEM_PRIMARY: Color32 = Color32::from_rgb(100, 160, 255);
/// Green — live indicators, success, meter low-band.
pub const SEM_SUCCESS: Color32 = Color32::from_rgb(80, 200, 140);
/// Amber — overrides, warnings, portal peer.
pub const SEM_WARNING: Color32 = Color32::from_rgb(220, 150, 40);
/// Brighter amber for warning icons that need to pop against dark chrome.
pub const SEM_WARNING_BRIGHT: Color32 = Color32::from_rgb(255, 180, 60);
/// Red — danger, clip, untyped / unknown.
pub const SEM_DANGER: Color32 = Color32::from_rgb(230, 80, 70);

// ---------------------------------------------------------------------------
// Data-type palette — one colour per PortType (consumed by `PortTypeUi::color`)
// ---------------------------------------------------------------------------

pub const TYPE_LOGIC: Color32 = Color32::from_rgb(240, 200, 40);
pub const TYPE_PHASE: Color32 = Color32::from_rgb(180, 100, 220);
pub const TYPE_COLOR: Color32 = Color32::from_rgb(60, 200, 220);
pub const TYPE_POSITION: Color32 = SEM_SUCCESS;
pub const TYPE_PALETTE: Color32 = Color32::from_rgb(220, 180, 100);
pub const TYPE_GRADIENT: Color32 = Color32::from_rgb(40, 200, 180);
pub const TYPE_UNTYPED: Color32 = SEM_DANGER;
pub const TYPE_ANY: Color32 = TEXT_DIM;

// ---------------------------------------------------------------------------
// Special accent (neither state nor type)
// ---------------------------------------------------------------------------

/// Rose/magenta — "this is a container" accent for Subgraph nodes.
pub const ACCENT_SUBGRAPH: Color32 = Color32::from_rgb(190, 90, 150);

// ---------------------------------------------------------------------------
// Alpha overlays (premultiplied form required for const)
// ---------------------------------------------------------------------------

/// Selection rectangle fill — semi-transparent primary.
pub const SEM_PRIMARY_FILL: Color32 = Color32::from_rgba_premultiplied(100, 160, 255, 30);
/// Wash drawn on top of overridden controls.
pub const SEM_WARNING_FILL: Color32 = Color32::from_rgba_premultiplied(220, 150, 40, 130);
/// Halo behind portal-peer nodes — premultiplied form of `SEM_WARNING` at α40.
pub const SEM_WARNING_HALO: Color32 = Color32::from_rgba_premultiplied(35, 24, 6, 40);
/// White halo behind selected wires — premultiplied form of white at α180.
pub const WIRE_HALO: Color32 = Color32::from_rgba_premultiplied(180, 180, 180, 180);
/// Node drop shadow.
pub const SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 60);
/// Wash drawn on top of disabled nodes to flatten them.
pub const DISABLED_WASH: Color32 = Color32::from_rgba_premultiplied(25, 25, 26, 225);
/// Darker band drawn over peak-meter bars to indicate RMS level.
pub const RMS_WASH: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 128);
