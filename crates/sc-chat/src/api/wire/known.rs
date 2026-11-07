use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

pub fn known<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.and_then(|raw| serde_json::from_value(raw).ok()))
}
