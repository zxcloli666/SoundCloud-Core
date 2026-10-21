use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewMode {
    Full,
    NameOnly,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPrefs {
    pub enabled: bool,
    pub preview: PreviewMode,
    pub sound: bool,
    pub stream_mode: bool,
}

impl Default for NotifyPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            preview: PreviewMode::Full,
            sound: true,
            stream_mode: false,
        }
    }
}
