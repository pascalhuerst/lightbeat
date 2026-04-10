use super::universe::DmxUniverse;

/// Configuration for a DMX output target.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OutputConfig {
    /// Art-Net output to a network destination.
    ArtNet {
        host: String,
        port: u16,
    },
    /// sACN (E1.31) output.
    #[allow(dead_code)]
    Sacn {
        host: String,
    },
    /// No output (preview only).
    None,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig::None
    }
}

/// Trait for sending DMX universe data to hardware.
///
/// Implementations will handle the actual network protocol (Art-Net, sACN, etc.).
/// For now this defines the interface; actual implementations come later when
/// the artnet/sacn crate dependencies are added.
pub trait Output: Send {
    /// Send a full universe frame to the output.
    fn send_universe(&mut self, universe: &DmxUniverse) -> Result<(), String>;

    /// Human-readable name for this output.
    fn name(&self) -> &str;
}

/// A placeholder output that discards all data. Useful for testing and preview mode.
pub struct NullOutput;

impl Output for NullOutput {
    fn send_universe(&mut self, _universe: &DmxUniverse) -> Result<(), String> {
        Ok(())
    }

    fn name(&self) -> &str {
        "None"
    }
}
