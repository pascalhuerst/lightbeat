//! cpal-backed audio input device.
//!
//! Converts the sample format (i16/u16 → f32) and mono-mixes channels inside
//! the cpal callback; subscribers always receive `Arc<[f32]>` mono chunks via
//! the shared `dispatch_samples` helper.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, Host, SampleFormat, StreamConfig, SupportedStreamConfig};
use crossbeam_channel::Sender;

use super::backend::{
    AudioBackend, AudioBackendKind, AudioChunk, DEFAULT_SAMPLE_RATE, InputStream, StreamRequest,
    dispatch_samples,
};

pub struct CpalBackend;

impl AudioBackend for CpalBackend {
    fn kind(&self) -> AudioBackendKind { AudioBackendKind::Cpal }

    fn list_input_names(&self) -> Vec<String> {
        let Ok(devices) = host().input_devices() else { return Vec::new(); };
        devices.filter_map(|d| d.name().ok()).collect()
    }

    fn open_input(
        &self,
        device_name: &str,
        request: StreamRequest,
        subscribers: Vec<Sender<AudioChunk>>,
    ) -> Result<InputStream, String> {
        let device = find_input_by_name(device_name)
            .ok_or_else(|| format!("cpal: device '{}' not found", device_name))?;
        open_input(&device, request, subscribers)
    }
}

fn host() -> Host { cpal::default_host() }

fn find_input_by_name(name: &str) -> Option<Device> {
    host().input_devices().ok()?
        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
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

fn open_input(
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

    let err_fn = |e| eprintln!("[audio] cpal stream error: {e}");
    let ch = channels as usize;

    let stream = match sample_format {
        SampleFormat::F32 => {
            let subs = subscribers;
            let produced_cb = produced.clone();
            let last_cb = last_chunk.clone();
            device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        last_cb.store(data.len() / ch.max(1), Ordering::Relaxed);
                        dispatch_samples(data, ch, &subs, &produced_cb, |s| s);
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
                        last_cb.store(data.len() / ch.max(1), Ordering::Relaxed);
                        dispatch_samples(data, ch, &subs, &produced_cb, |s| s as f32 / 32768.0);
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
                        last_cb.store(data.len() / ch.max(1), Ordering::Relaxed);
                        dispatch_samples(data, ch, &subs, &produced_cb, |s| (s as f32 - 32768.0) / 32768.0);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        other => return Err(format!("unsupported sample format: {other:?}")),
    };
    stream.play().map_err(|e| e.to_string())?;

    Ok(InputStream::from_parts(
        Box::new(stream),
        sample_rate.0,
        channels,
        produced,
        last_chunk,
    ))
}
