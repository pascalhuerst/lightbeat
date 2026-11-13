//! Helpers for the serde config structs that nodes save and that their
//! widgets push through `pending_config`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

/// For a list field: an entry that does not parse is dropped and the others
/// are kept, which is how the hand-written readers behaved.
pub fn lenient_vec<'de, D, T>(d: D) -> Result<Option<Vec<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(d)?;
    Ok(raw.map(|v| v.into_iter().filter_map(|e| serde_json::from_value(e).ok()).collect()))
}
