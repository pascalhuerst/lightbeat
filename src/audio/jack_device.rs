//! JACK-backed audio input device.
//!
//! The `jack` crate is a thin binding over libjack. On a pipewire system
//! (pipewire-jack is installed), this routes through pipewire's JACK
//! interface — a genuinely different code path from cpal's ALSA route.
//!
//! JACK's "device" concept is port-based: physical capture sources appear as
//! `IS_OUTPUT | IS_PHYSICAL` ports (from their perspective, they OUTPUT audio
//! into the JACK graph). We register our own input port on a disposable
//! client and connect the chosen source port to it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crossbeam_channel::Sender;
use jack::{
    AudioIn, Client, ClientOptions, Control, PortFlags, ProcessScope,
    contrib::ClosureProcessHandler,
};

use super::backend::{
    AudioBackend, AudioBackendKind, AudioChunk, InputStream, StreamRequest,
};

pub struct JackBackend;

impl AudioBackend for JackBackend {
    fn kind(&self) -> AudioBackendKind { AudioBackendKind::Jack }

    fn list_input_names(&self) -> Vec<String> {
        // Spin up a disposable client just to walk the port graph. No server
        // auto-start — if there's no JACK server (or no pipewire-jack), we
        // silently return an empty list so the UI degrades gracefully.
        let Ok((client, _status)) = Client::new("lightbeat-probe", ClientOptions::NO_START_SERVER)
        else {
            return Vec::new();
        };
        client.ports(
            None,
            Some("32 bit float mono audio"),
            PortFlags::IS_OUTPUT | PortFlags::IS_PHYSICAL,
        )
    }

    fn open_input(
        &self,
        device_name: &str,
        _request: StreamRequest,
        subscribers: Vec<Sender<AudioChunk>>,
    ) -> Result<InputStream, String> {
        let (client, _status) = Client::new("lightbeat-capture", ClientOptions::NO_START_SERVER)
            .map_err(|e| format!("jack client: {}", e))?;

        // JACK server owns sample rate and buffer size — the requested values
        // on `StreamRequest` are informational only.
        let sample_rate = client.sample_rate() as u32;

        let in_port = client
            .register_port("in", AudioIn::default())
            .map_err(|e| format!("register_port: {}", e))?;
        let in_port_name = in_port.name().map_err(|e| format!("port name: {}", e))?;

        let produced = Arc::new(AtomicU64::new(0));
        let last_chunk = Arc::new(AtomicUsize::new(0));
        let produced_cb = produced.clone();
        let last_cb = last_chunk.clone();

        let process = ClosureProcessHandler::new(move |_client: &Client, ps: &ProcessScope| {
            let buffer = in_port.as_slice(ps);
            last_cb.store(buffer.len(), Ordering::Relaxed);
            // JACK already delivers 32-bit float mono per port, so we can
            // dispatch directly — no mono-mix needed.
            let start = produced_cb.fetch_add(buffer.len() as u64, Ordering::Relaxed);
            let chunk = AudioChunk {
                start_sample: start,
                mono_f32: Arc::from(buffer.to_vec().into_boxed_slice()),
            };
            for s in &subscribers {
                let _ = s.try_send(chunk.clone());
            }
            Control::Continue
        });

        let active = client
            .activate_async((), process)
            .map_err(|e| format!("activate: {}", e))?;

        // Wire the chosen physical source into our capture port. This must
        // happen after activation so the port is live in the JACK graph.
        active
            .as_client()
            .connect_ports_by_name(device_name, &in_port_name)
            .map_err(|e| format!("connect '{}' → '{}': {}", device_name, in_port_name, e))?;

        Ok(InputStream::from_parts(
            Box::new(active),
            sample_rate,
            1,
            produced,
            last_chunk,
        ))
    }
}
