use sacn::DmxSource;

use crate::objects::universe::DmxUniverse;
use super::DmxOutput;

/// sACN (E1.31) DMX output. Sends DMX universes via multicast using the
/// ANSI E1.31 Streaming ACN protocol.
pub struct SacnOutput {
    source: DmxSource,
    source_name: String,
}

impl SacnOutput {
    /// Create a new sACN output with the given source name.
    ///
    /// The source name is visible to sACN receivers on the network.
    pub fn new(source_name: &str) -> Result<Self, String> {
        let source = DmxSource::new(source_name)
            .map_err(|e| format!("Failed to create sACN source: {}", e))?;

        Ok(Self {
            source,
            source_name: source_name.to_string(),
        })
    }
}

impl DmxOutput for SacnOutput {
    fn send_universe(&mut self, universe: &DmxUniverse) -> Result<(), String> {
        // sACN universes are 1-based (1–63999).
        // Map from Art-Net addressing: universe number = (net * 256) + (subnet * 16) + universe + 1
        let sacn_universe = (universe.net as u16) * 256
            + (universe.subnet as u16) * 16
            + (universe.universe as u16)
            + 1; // sACN is 1-based

        self.source
            .send(sacn_universe, &universe.channels)
            .map_err(|e| format!("Failed to send sACN universe {}: {}", sacn_universe, e))?;

        Ok(())
    }

    fn name(&self) -> &str {
        &self.source_name
    }
}

impl Drop for SacnOutput {
    fn drop(&mut self) {
        // sACN protocol requires sending termination packets when a source goes away.
        // Best-effort: ignore errors during cleanup.
        // We'd need to track which universes we've sent to, so for now this is a no-op.
        // Individual universe streams can be terminated with source.terminate_stream(universe).
    }
}
