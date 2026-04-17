//! Audio input + analyzer subsystem.
//!
//! - `device` wraps cpal: enumerates inputs, opens streams, mono-mixes to f32
//!   and fans out to per-analyzer subscribers.
//! - `analyzers` contains the analyzer implementations (currently just beat
//!   detection). Each analyzer runs on its own thread, consumes f32 audio
//!   chunks, writes its outputs into shared state.
//! - `manager` owns the active streams + analyzer threads and exposes shared
//!   state to the rest of the app.

pub mod analyzers;
pub mod device;
pub mod manager;
