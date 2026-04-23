//! Audio input backend abstraction.
//!
//! Each backend (cpal, pipewire, alsa, …) lists its available input devices
//! by human-readable name and opens a mono-mixed f32 stream that fans chunks
//! out to subscribers. The manager stores a `(backend, device_name)` pair per
//! input so users can mix backends freely — e.g. one input via cpal, another
//! via pipewire — if the default path is misbehaving for them.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crossbeam_channel::{Receiver, Sender, bounded};
use serde::{Deserialize, Serialize};

/// Sensible default sample-rate request when nothing is configured.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Which audio backend to use for a given input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AudioBackendKind {
    Cpal,
    // Pipewire, Alsa — to be added.
}

impl Default for AudioBackendKind {
    fn default() -> Self { AudioBackendKind::Cpal }
}

impl AudioBackendKind {
    pub const ALL: &'static [AudioBackendKind] = &[AudioBackendKind::Cpal];

    pub fn label(self) -> &'static str {
        match self {
            AudioBackendKind::Cpal => "cpal",
        }
    }
}

/// Open-stream configuration (request-only; the backend may negotiate
/// different values).
#[derive(Debug, Clone, Copy)]
pub struct StreamRequest {
    pub sample_rate: Option<u32>,
    /// If None, the backend picks a buffer size.
    pub buffer_size_frames: Option<u32>,
}

impl Default for StreamRequest {
    fn default() -> Self {
        Self {
            sample_rate: Some(DEFAULT_SAMPLE_RATE),
            buffer_size_frames: None,
        }
    }
}

/// Chunk of audio samples with a monotonic stream-time index
/// (samples @ stream_rate).
#[derive(Clone)]
pub struct AudioChunk {
    pub start_sample: u64,
    pub mono_f32: Arc<[f32]>,
}

/// Live input stream. Drop to stop.
///
/// The backend-specific stream handle is held inside `_handle` as an opaque
/// `Any`: its only purpose is keeping the underlying OS stream alive — no
/// user-facing code should downcast it. Not `Send`, because cpal's `Stream`
/// is `!Send` on some platforms; the manager holds streams on the UI thread
/// so that's fine.
pub struct InputStream {
    _handle: Box<dyn std::any::Any>,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples_produced: Arc<AtomicU64>,
    /// Most recent backend-callback buffer size (frames). 0 until first callback.
    pub last_chunk_frames: Arc<AtomicUsize>,
}

impl InputStream {
    pub fn observed_chunk_frames(&self) -> usize {
        self.last_chunk_frames.load(Ordering::Relaxed)
    }

    /// Backend-side constructor — wraps the opaque stream handle together
    /// with the shared counters. Not for use outside backend implementations.
    pub fn from_parts(
        handle: Box<dyn std::any::Any>,
        sample_rate: u32,
        channels: u16,
        samples_produced: Arc<AtomicU64>,
        last_chunk_frames: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            _handle: handle,
            sample_rate,
            channels,
            samples_produced,
            last_chunk_frames,
        }
    }
}

/// Trait implemented by every audio-input backend. All implementations fan
/// out mono f32 `AudioChunk`s; all sample-format conversion and channel
/// mono-mixing happens inside the backend's own callback.
pub trait AudioBackend: Send + Sync {
    fn kind(&self) -> AudioBackendKind;

    /// Human-readable names of available input devices.
    fn list_input_names(&self) -> Vec<String>;

    /// Open an input stream on the named device.
    fn open_input(
        &self,
        device_name: &str,
        request: StreamRequest,
        subscribers: Vec<Sender<AudioChunk>>,
    ) -> Result<InputStream, String>;
}

/// Return the backend implementation for the given kind. Static — backends
/// are stateless handles that probe devices each call.
pub fn backend_for(kind: AudioBackendKind) -> &'static dyn AudioBackend {
    match kind {
        AudioBackendKind::Cpal => &crate::audio::cpal_device::CpalBackend,
    }
}

/// Bounded subscriber pair. Bounded so the audio callback can drop frames
/// cheaply if a slow analyzer falls behind, instead of blocking.
pub fn mk_subscriber() -> (Sender<AudioChunk>, Receiver<AudioChunk>) {
    bounded(256)
}

/// Shared mono-mix + dispatch used by backend implementations. `data` is
/// interleaved samples, `channels` frames per sample. Samples are averaged
/// into mono f32, then broadcast to every subscriber.
pub fn dispatch_samples<T, F>(
    data: &[T],
    channels: usize,
    subs: &[Sender<AudioChunk>],
    produced: &AtomicU64,
    mut to_f32: F,
)
where
    F: FnMut(T) -> f32,
    T: Copy,
{
    let c = channels.max(1);
    let mono: Vec<f32> = data
        .chunks_exact(c)
        .map(|frame| {
            let mut sum = 0.0;
            for &s in frame { sum += to_f32(s); }
            sum / c as f32
        })
        .collect();
    let start = produced.fetch_add(mono.len() as u64, Ordering::Relaxed);
    let chunk = AudioChunk {
        start_sample: start,
        mono_f32: Arc::from(mono.into_boxed_slice()),
    };
    for s in subs {
        let _ = s.try_send(chunk.clone());
    }
}
