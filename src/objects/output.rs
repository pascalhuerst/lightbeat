/// Configuration identifying which DMX output a fixture is assigned to.
///
/// This is stored on the fixture itself. The actual output implementation
/// lives in `crate::interfaces`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OutputConfig {
    /// Art-Net output.
    ArtNet {
        host: String,
        port: u16,
    },
    /// sACN (E1.31) output.
    Sacn {
        source_name: String,
    },
    /// No output (preview only).
    None,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig::None
    }
}
