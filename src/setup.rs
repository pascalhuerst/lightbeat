use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::audio::manager::AudioInputConfig;
use crate::dmx_input::DmxInputConfig;
use crate::input_controller::InputController;
use crate::objects::color_palette::{ColorPalette, ColorPaletteGroup};
use crate::objects::fixture::Fixture;
use crate::objects::gradient_preset::GradientPreset;
use crate::objects::group::Group;
use crate::objects::object::Object;
use crate::objects::output::OutputConfig;

const SETUP_FILENAME: &str = "setup.json";

// ---------------------------------------------------------------------------
// Setup file format
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct SetupFile {
    /// Fixture templates (channel definitions).
    pub fixtures: Vec<Fixture>,
    /// Object instances (fixture type + address + interface).
    #[serde(default)]
    pub objects: Vec<Object>,
    pub interfaces: Vec<SavedInterface>,
    #[serde(default)]
    pub groups: Vec<Group>,
    /// Color palettes (each palette is a set of 4 colors).
    #[serde(default)]
    pub color_palettes: Vec<ColorPalette>,
    /// Groups of palettes.
    #[serde(default)]
    pub color_palette_groups: Vec<ColorPaletteGroup>,
    /// Reusable gradient presets, picked from the Gradient Source's "Load
    /// preset" dropdown. Templates only — copying a preset onto a node
    /// doesn't create a live link.
    #[serde(default)]
    pub gradient_presets: Vec<GradientPreset>,
    /// Input controllers (MIDI etc.) — virtual slots that bind to hardware.
    #[serde(default)]
    pub input_controllers: Vec<InputController>,
    /// Audio inputs + per-input analyzer lists.
    #[serde(default)]
    pub audio_inputs: Vec<AudioInputConfig>,
    /// DMX coming in from other lighting desks.
    #[serde(default)]
    pub dmx_inputs: Vec<DmxInputConfig>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct SavedInterface {
    pub id: u32,
    pub name: String,
    pub config: OutputConfig,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn setup_path() -> PathBuf {
    crate::paths::data_file(SETUP_FILENAME)
}

/// Write the setup unless `blocked`. A setup that failed to load starts the
/// session empty, and writing that over the unreadable file would destroy it.
/// Returns whether anything was written.
pub fn save_setup(setup: &SetupFile, blocked: bool) -> Result<bool, String> {
    write_setup(&setup_path(), setup, blocked)
}

fn write_setup(path: &std::path::Path, setup: &SetupFile, blocked: bool) -> Result<bool, String> {
    if blocked {
        return Ok(false);
    }
    let json = serde_json::to_string_pretty(setup).map_err(|e| e.to_string())?;
    crate::paths::write_atomic(path, json).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Keep an unreadable setup file by renaming it to `<name>.bad`, so the user
/// can recover it by hand.
pub fn set_aside_bad_setup() -> Result<PathBuf, String> {
    set_aside(&setup_path())
}

fn set_aside(path: &std::path::Path) -> Result<PathBuf, String> {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".bad");
    let bad = path.with_file_name(name);
    std::fs::rename(path, &bad).map_err(|e| e.to_string())?;
    Ok(bad)
}

pub fn load_setup() -> Result<SetupFile, String> {
    let path = setup_path();
    if !path.exists() {
        return Ok(SetupFile::default());
    }
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Moving a setup between computers
// ---------------------------------------------------------------------------

/// Write the setup to any file, for taking it to another computer.
pub fn export_to(setup: &SetupFile, path: &std::path::Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(setup).map_err(|e| e.to_string())?;
    crate::paths::write_atomic(path, json).map_err(|e| e.to_string())
}

pub fn import_from(path: &std::path::Path) -> Result<SetupFile, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| format!("not a LightBeat setup: {e}"))
}

/// What this computer has that a setup refers to by name.
#[derive(Default, Clone)]
pub struct Machine {
    pub audio_backends: Vec<crate::audio::backend::AudioBackendKind>,
    pub audio_devices: std::collections::HashMap<crate::audio::backend::AudioBackendKind, Vec<String>>,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
    pub network_addresses: Vec<String>,
}

impl Machine {
    pub fn probe() -> Self {
        use crate::audio::backend::{AudioBackendKind, backend_for};
        let audio_backends = AudioBackendKind::available();
        let audio_devices = audio_backends.iter().map(|k| (*k, backend_for(*k).list_input_names())).collect();
        Self {
            audio_backends,
            audio_devices,
            midi_inputs: crate::input_controller::midi::available_ports(),
            midi_outputs: crate::input_controller::midi::available_output_ports(),
            network_addresses: crate::dmx_input::artnet::local_interfaces().iter().map(|i| i.ip.to_string()).collect(),
        }
    }
}

/// Point what a setup refers to by name at what this computer has, and say
/// what could not be matched. Audio inputs fall back to the system default
/// input (a device name means nothing on another machine); MIDI ports are
/// found again under the name this system gives them; a controller whose
/// hardware is not plugged in stays as it is and waits for it.
pub fn adapt_to_machine(setup: &mut SetupFile, machine: &Machine) -> Vec<String> {
    use crate::audio::backend::AudioBackendKind;
    use crate::input_controller::InputControllerKind as K;
    use crate::input_controller::midi::resolve_port;

    let mut notes = Vec::new();

    for input in &mut setup.audio_inputs {
        if !machine.audio_backends.contains(&input.backend) {
            notes.push(format!("Audio input \"{}\" used {}, which is not available here; it uses {} now.", input.name, input.backend.label(), AudioBackendKind::Cpal.label()));
            input.backend = AudioBackendKind::Cpal;
            if !input.device_name.is_empty() {
                input.device_name = crate::audio::cpal_device::DEFAULT_INPUT.to_string();
            }
            continue;
        }
        if input.device_name.is_empty() {
            continue;
        }
        let known = machine.audio_devices.get(&input.backend).is_some_and(|d| d.contains(&input.device_name));
        if known {
            continue;
        }
        if input.backend == AudioBackendKind::Cpal {
            notes.push(format!("Audio input \"{}\": \"{}\" is not on this computer; it listens to the system default input. Pick another device in Setup, Audio Inputs.", input.name, input.device_name));
            input.device_name = crate::audio::cpal_device::DEFAULT_INPUT.to_string();
        } else {
            notes.push(format!("Audio input \"{}\": \"{}\" is not on this computer. Pick a device in Setup, Audio Inputs.", input.name, input.device_name));
            input.device_name.clear();
        }
    }

    for c in &mut setup.input_controllers {
        let name = c.name.clone();
        let mut fix = |port: &mut String, available: &[String], what: &str| {
            if port.is_empty() || available.contains(port) {
                return;
            }
            match resolve_port(port, available) {
                Some(found) => *port = found,
                None => notes.push(format!("Controller \"{name}\": {what} port \"{port}\" is not connected; it connects when the device is plugged in and Rescan is pressed.")),
            }
        };
        match &mut c.kind {
            K::Midi { hw_port_name } => fix(hw_port_name, &machine.midi_inputs, "MIDI"),
            K::Bcf2000 { hw_input_port, hw_output_port }
            | K::Push1 { hw_input_port, hw_output_port }
            | K::LaunchpadS { hw_input_port, hw_output_port } => {
                fix(hw_input_port, &machine.midi_inputs, "MIDI input");
                fix(hw_output_port, &machine.midi_outputs, "MIDI output");
            }
            K::X1 => {}
        }
    }

    for input in &mut setup.dmx_inputs {
        let crate::dmx_input::DmxInputKind::ArtNet { interface, .. } = &mut input.kind;
        if !interface.is_empty() && !machine.network_addresses.contains(interface) {
            notes.push(format!("DMX input \"{}\": the network {interface} is not on this computer; it announces on all networks.", input.name));
            interface.clear();
        }
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::backend::AudioBackendKind;
    use crate::audio::manager::AudioInputConfig;
    use crate::input_controller::{InputController, InputControllerKind};

    #[test]
    fn a_blocked_setup_is_never_written() {
        let dir = std::env::temp_dir().join(format!("lightbeat-setup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setup.json");
        std::fs::write(&path, "original").unwrap();

        assert_eq!(write_setup(&path, &SetupFile::default(), true), Ok(false));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");

        assert_eq!(write_setup(&path, &SetupFile::default(), false), Ok(true));
        assert_ne!(std::fs::read_to_string(&path).unwrap(), "original");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_setup_is_kept_next_to_where_the_new_one_goes() {
        let dir = std::env::temp_dir().join(format!("lightbeat-setup-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setup.json");
        std::fs::write(&path, "{ not json").unwrap();

        let bad = set_aside(&path).unwrap();
        assert_eq!(bad, dir.join("setup.json.bad"));
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), "{ not json");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn mac() -> Machine {
        let mut m = Machine::default();
        m.audio_backends = vec![AudioBackendKind::Cpal];
        m.audio_devices.insert(AudioBackendKind::Cpal, vec!["(system default input)".into(), "MacBook Pro Microphone".into()]);
        m.midi_inputs = vec!["BCF2000 Port 1".into()];
        m.midi_outputs = vec!["BCF2000 Port 1".into()];
        m.network_addresses = vec!["192.168.1.20".into()];
        m
    }

    fn audio(name: &str, backend: AudioBackendKind, device: &str) -> AudioInputConfig {
        AudioInputConfig {
            id: 1, name: name.into(), enabled: true, backend, device_name: device.into(),
            sample_rate: None, buffer_size_frames: None, analyzers: vec![],
        }
    }

    #[test]
    fn a_setup_from_linux_lands_on_the_defaults_of_a_mac() {
        let mut setup = SetupFile::default();
        setup.audio_inputs = vec![
            audio("Master", AudioBackendKind::Cpal, "alsa_input.usb-Focusrite"),
            audio("Jack", AudioBackendKind::Jack, "system:capture_1"),
            audio("Here", AudioBackendKind::Cpal, "MacBook Pro Microphone"),
            audio("None", AudioBackendKind::Cpal, ""),
        ];
        setup.input_controllers = vec![
            InputController { id: 1, name: "Faders".into(), kind: InputControllerKind::Bcf2000 { hw_input_port: "BCF2000:BCF2000 MIDI 1 28:0".into(), hw_output_port: "BCF2000:BCF2000 MIDI 1 28:0".into() }, inputs: vec![] },
            InputController { id: 2, name: "Pad".into(), kind: InputControllerKind::LaunchpadS { hw_input_port: "Launchpad S:Launchpad S MIDI 1 20:0".into(), hw_output_port: String::new() }, inputs: vec![] },
        ];
        let notes = adapt_to_machine(&mut setup, &mac());
        assert_eq!(setup.audio_inputs[0].device_name, "(system default input)");
        assert_eq!(setup.audio_inputs[1].backend, AudioBackendKind::Cpal);
        assert_eq!(setup.audio_inputs[2].device_name, "MacBook Pro Microphone");
        assert_eq!(setup.audio_inputs[3].device_name, "");
        match &setup.input_controllers[0].kind {
            InputControllerKind::Bcf2000 { hw_input_port, hw_output_port } => {
                assert_eq!((hw_input_port.as_str(), hw_output_port.as_str()), ("BCF2000 Port 1", "BCF2000 Port 1"));
            }
            _ => unreachable!(),
        }
        assert_eq!(notes.len(), 3, "{notes:?}");
        assert!(notes.iter().any(|n| n.contains("Pad")), "the Launchpad is not plugged in");
    }

    #[test]
    fn a_dmx_input_on_a_network_that_is_not_here_announces_everywhere() {
        let mut setup = SetupFile::default();
        let mut input = crate::dmx_input::DmxInputConfig::new_art_net(1, "Desk");
        input.kind = crate::dmx_input::DmxInputKind::ArtNet { universes: vec![0], interface: "10.0.0.5".into() };
        setup.dmx_inputs = vec![input];
        let notes = adapt_to_machine(&mut setup, &mac());
        assert_eq!(setup.dmx_inputs[0].interface(), "");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn an_exported_setup_imports_unchanged() {
        let mut setup = SetupFile::default();
        setup.audio_inputs = vec![audio("Master", AudioBackendKind::Cpal, "x")];
        let dir = std::env::temp_dir().join(format!("lightbeat-setup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("setup-export.json");
        export_to(&setup, &path).unwrap();
        assert!(import_from(&path) == Ok(setup));
        std::fs::write(&path, "{\"hello\": 1").unwrap();
        assert!(import_from(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
