mod artnet;
mod sacn;

pub use artnet::ArtNetOutput;
pub use sacn::SacnOutput;

use crate::objects::DmxUniverse;

/// Trait for sending DMX universe data to hardware over a network protocol.
pub trait DmxOutput: Send {
    /// Send a full universe frame.
    fn send_universe(&mut self, universe: &DmxUniverse) -> Result<(), String>;

    /// Human-readable name for this output.
    fn name(&self) -> &str;
}

/// Configuration for creating a DMX output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DmxOutputConfig {
    /// Art-Net output.
    ArtNet {
        /// Target host (IP address or hostname). Use "255.255.255.255" for broadcast.
        host: String,
        /// Art-Net port (default: 6454).
        port: u16,
    },
    /// sACN / E1.31 output.
    Sacn {
        /// Source name visible to receivers.
        source_name: String,
    },
}

impl DmxOutputConfig {
    pub fn artnet_broadcast() -> Self {
        DmxOutputConfig::ArtNet {
            host: "255.255.255.255".to_string(),
            port: 6454,
        }
    }

    pub fn artnet_unicast(host: impl Into<String>) -> Self {
        DmxOutputConfig::ArtNet {
            host: host.into(),
            port: 6454,
        }
    }

    pub fn sacn(source_name: impl Into<String>) -> Self {
        DmxOutputConfig::Sacn {
            source_name: source_name.into(),
        }
    }

    /// Create a `DmxOutput` from this configuration.
    pub fn build(&self) -> Result<Box<dyn DmxOutput>, String> {
        match self {
            DmxOutputConfig::ArtNet { host, port } => {
                let output = ArtNetOutput::new(host, *port)?;
                Ok(Box::new(output))
            }
            DmxOutputConfig::Sacn { source_name } => {
                let output = SacnOutput::new(source_name)?;
                Ok(Box::new(output))
            }
        }
    }
}
