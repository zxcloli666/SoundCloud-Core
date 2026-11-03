use serde::{Deserialize, Serialize};

use super::ids::ConvId;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaConfig {
    pub uploads: bool,
    pub star: bool,
    pub quota: u64,
    pub used: u64,
    pub limits: MediaLimits,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaLimits {
    pub image_max_bytes: u32,
    pub thumb_max_bytes: u32,
    pub avatar_max_bytes: u32,
    pub album_max: u32,
    pub room_album_max: u32,
    pub restore_window_s: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineOrder {
    #[default]
    New,
    Old,
    Big,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MineQuery {
    #[serde(default)]
    pub order: MineOrder,
    #[serde(default)]
    pub conversation_id: Option<ConvId>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: u32,
}
