use serde::{Deserialize, Serialize};

use super::conversation::UserRef;
use super::embed::Embed;
use super::ids::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomPreview {
    pub code: String,
    pub title: Option<String>,
    pub host: UserRef,
    pub track: Option<Embed>,
    pub listeners: u32,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RoomModeration {
    Kick { user_id: UserId },
    Ban { user_id: UserId },
    Mute { user_id: UserId, seconds: u32 },
    Moderator { user_id: UserId, on: bool },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSettingsPatch {
    pub title: Option<String>,
    pub slow_mode_s: Option<u32>,
    pub chat_enabled: Option<bool>,
    pub links: Option<bool>,
}
