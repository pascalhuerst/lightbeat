//! Audio input + analyzer subsystem.
//!
//! - `backend` defines the `AudioBackend` trait, the `AudioBackendKind` enum,
//!   and the shared types (`AudioChunk`, `StreamRequest`, `InputStream`).
//! - `cpal_device` implements `AudioBackend` via cpal. Additional backends
//!   (pipewire, alsa) can be added as peer modules implementing the same
//!   trait.
//! - `analyzers` contains the analyzer implementations. Each analyzer runs
//!   on its own thread, consumes f32 audio chunks, writes outputs into
//!   shared state.
//! - `manager` owns the active streams + analyzer threads and exposes shared
//!   state to the rest of the app.

pub mod analyzers;
pub mod backend;
pub mod cpal_device;
pub mod manager;
