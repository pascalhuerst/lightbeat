//! cpal-backed audio input device wrapper.
//!
//! All sample-format conversion (i16/u16 → f32) and channel mono-mixing
//! happens inside the cpal callback in this module. Subscribers always
//! receive `Arc<[f32]>` mono audio chunks.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, Host, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
use crossbeam_channel::{Receiver, Sender, bounded};

/// Common request fallback when nothing is configured.
pub const DEFAULT_SAMPLE_RATE: u32 = 48_000;

/// Information about an available input device on the system.
pub struct DeviceInfo {
    pub name: String,
    pub device: Device,
}

pub fn host() -> Host {
    cpal::default_host()
}

pub fn list_inputs() -> Vec<DeviceInfo> {
    let Ok(devices) = host().input_devices() else {
        return vec![];
    };
    devices
        .filter_map(|d| {
            let name = d.name().ok()?;
            Some(DeviceInfo { name, device: d })
        })
        .collect()
}

//pub fn list_input_names() -> Vec<String> {
//    list_inputs().into_iter().map(|d| d.name).collect()
//}

pub fn find_input_by_name(name: &str) -> Option<Device> {
    list_inputs()
        .into_iter()
        .find(|d| d.name == name)
        .map(|d| d.device)
}

/// Sample rates supported by a device for input. Returns at least the device
/// default if nothing else is reported.
pub fn supported_sample_rates(device: &Device) -> Vec<u32> {
    let mut rates: Vec<u32> = Vec::new();
    if let Ok(configs) = device.supported_input_configs() {
        for c in configs {
            for candidate in [44_100, 48_000, 88_200, 96_000, 192_000] {
                let sr = cpal::SampleRate(candidate);
                if c.min_sample_rate() <= sr
                    && c.max_sample_rate() >= sr
                    && !rates.contains(&candidate)
                {
                    rates.push(candidate);
                }
            }
        }
    }
    if rates.is_empty() {
        if let Ok(def) = device.default_input_config() {
            rates.push(def.sample_rate().0);
        } else {
            rates.push(DEFAULT_SAMPLE_RATE);
        }
    }
    rates.sort_unstable();
    rates
}

/// Chunk of audio samples with a monotonic stream-time index (samples @ stream_rate).
#[derive(Clone)]
pub struct AudioChunk {
    pub start_sample: u64,
    pub mono_f32: Arc<[f32]>,
}

/// Open-stream configuration (request-only; cpal may negotiate different values).
#[derive(Debug, Clone, Copy)]
pub struct StreamRequest {
    pub sample_rate: Option<u32>,
    /// If None, cpal picks a buffer size.
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

/// Live input stream. Drop to stop.
pub struct InputStream {
    _stream: Stream,
    pub sample_rate: u32,
    pub channels: u16,
    pub samples_produced: Arc<AtomicU64>,
    /// Most recent cpal callback's buffer size (frames). 0 until first callback.
    pub last_chunk_frames: Arc<AtomicUsize>,
}

impl InputStream {
    pub fn observed_chunk_frames(&self) -> usize {
        self.last_chunk_frames.load(Ordering::Relaxed)
    }
}

fn pick_config(device: &Device, target_rate: u32) -> Option<SupportedStreamConfig> {
    let configs = device.supported_input_configs().ok()?;
    configs
        .filter(|c| {
            c.min_sample_rate() <= cpal::SampleRate(target_rate)
                && c.max_sample_rate() >= cpal::SampleRate(target_rate)
        })
        .min_by_key(|c| {
            let fmt_penalty = match c.sample_format() {
                SampleFormat::F32 => 0,
                SampleFormat::I16 => 1,
                SampleFormat::U16 => 2,
                _ => 3,
            };
            (fmt_penalty, c.channels())
        })
        .and_then(|c| c.try_with_sample_rate(cpal::SampleRate(target_rate)))
}

/// Open an input stream on the given device. Mono-mixes all channels and fans
/// out f32 chunks to each subscriber.
pub fn open_input(
    device: &Device,
    request: StreamRequest,
    subscribers: Vec<Sender<AudioChunk>>,
) -> Result<InputStream, String> {
    let target_rate = request.sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE);
    let supported = pick_config(device, target_rate)
        .or_else(|| device.default_input_config().ok())
        .ok_or_else(|| "no input config available".to_string())?;
    let sample_format = supported.sample_format();
    let channels = supported.channels();
    let sample_rate = supported.sample_rate();
    let mut config: StreamConfig = supported.into();
    if let Some(bs) = request.buffer_size_frames {
        config.buffer_size = BufferSize::Fixed(bs);
    }

    let produced = Arc::new(AtomicU64::new(0));
    let last_chunk = Arc::new(AtomicUsize::new(0));

    let err_fn = |e| eprintln!("[audio] stream error: {e}");

    let stream = match sample_format {
        SampleFormat::F32 => {
            let subs = subscribers;
            let produced_cb = produced.clone();
            let last_cb = last_chunk.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        last_cb.store(data.len() / channels.max(1) as usize, Ordering::Relaxed);
                        on_samples_f32(data, channels as usize, &subs, &produced_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::I16 => {
            let subs = subscribers;
            let produced_cb = produced.clone();
            let last_cb = last_chunk.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        last_cb.store(data.len() / channels.max(1) as usize, Ordering::Relaxed);
                        on_samples_i16(data, channels as usize, &subs, &produced_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        SampleFormat::U16 => {
            let subs = subscribers;
            let produced_cb = produced.clone();
            let last_cb = last_chunk.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        last_cb.store(data.len() / channels.max(1) as usize, Ordering::Relaxed);
                        on_samples_u16(data, channels as usize, &subs, &produced_cb);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;

    Ok(InputStream {
        _stream: stream,
        sample_rate: sample_rate.0,
        channels,
        samples_produced: produced,
        last_chunk_frames: last_chunk,
    })
}

fn on_samples_f32(
    data: &[f32],
    channels: usize,
    subs: &[Sender<AudioChunk>],
    produced: &AtomicU64,
) {
    let c = channels.max(1);
    let mono: Vec<f32> = data
        .chunks_exact(c)
        .map(|frame| frame.iter().copied().sum::<f32>() / c as f32)
        .collect();
    dispatch(subs, produced, mono);
}

fn on_samples_i16(
    data: &[i16],
    channels: usize,
    subs: &[Sender<AudioChunk>],
    produced: &AtomicU64,
) {
    let c = channels.max(1);
    let mono: Vec<f32> = data
        .chunks_exact(c)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|&s| s as i32).sum::<i32>();
            (sum as f32 / c as f32) / 32768.0
        })
        .collect();
    dispatch(subs, produced, mono);
}

fn on_samples_u16(
    data: &[u16],
    channels: usize,
    subs: &[Sender<AudioChunk>],
    produced: &AtomicU64,
) {
    let c = channels.max(1);
    let mono: Vec<f32> = data
        .chunks_exact(c)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|&s| s as i32 - 32768).sum::<i32>();
            (sum as f32 / c as f32) / 32768.0
        })
        .collect();
    dispatch(subs, produced, mono);
}

fn dispatch(subs: &[Sender<AudioChunk>], produced: &AtomicU64, mono: Vec<f32>) {
    let start = produced.fetch_add(mono.len() as u64, Ordering::Relaxed);
    let chunk = AudioChunk {
        start_sample: start,
        mono_f32: Arc::from(mono.into_boxed_slice()),
    };
    for s in subs {
        let _ = s.try_send(chunk.clone());
    }
}

/// Create a bounded subscriber pair. Bounded so the audio callback can drop
/// frames cheaply if a slow analyzer falls behind, instead of blocking.
pub fn mk_subscriber() -> (Sender<AudioChunk>, Receiver<AudioChunk>) {
    bounded(256)
}
