use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use crate::contracts::MediaKind;
use crate::contracts::{MediaConfig, MediaLimits};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveMedia {
    pub upload_id: Uuid,
    pub conversation_id: Uuid,
    pub kind: MediaKind,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReserveAnswer {
    pub id: Uuid,
    pub put: PresignedPut,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<ReservedThumb>,
    pub expires_in_s: u32,
    pub used: u64,
    pub quota: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservedThumb {
    pub id: Uuid,
    pub put: PresignedPut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresignedPut {
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitAnswer {
    pub id: Uuid,
    pub bytes: u64,
    pub used: u64,
    pub quota: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<Uuid>,
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlsAnswer {
    pub expires_in_s: u32,
    pub items: Vec<MediaUrl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MediaUrl {
    Ok { id: Uuid, url: String },
    Deleted { id: Uuid },
    Lost { id: Uuid },
}

impl MediaUrl {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Ok { id, .. } | Self::Deleted { id } | Self::Lost { id } => *id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaUsage {
    pub bytes: u64,
    pub reserved: u64,
    pub quota: u64,
    pub files: u32,
    #[serde(default)]
    pub conversations: Vec<ConversationUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationUsage {
    pub conversation_id: Uuid,
    pub bytes: u64,
    pub files: u32,
    pub gone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinePage {
    pub items: Vec<MyMedia>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyMedia {
    pub id: Uuid,
    pub kind: MediaKind,
    pub conversation_id: Uuid,
    pub thumb_id: Option<Uuid>,
    pub bytes: u64,
    pub status: MyMediaStatus,
    pub chats: u32,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MyMediaStatus {
    Uploaded,
    Attached,
    Lost,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSelection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all: bool,
}

impl MediaSelection {
    pub fn ids(ids: Vec<Uuid>) -> Self {
        Self { ids, all: false }
    }

    pub fn all() -> Self {
        Self {
            ids: Vec::new(),
            all: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAnswer {
    pub deleted: u32,
    pub freed: u64,
    pub used: u64,
    pub quota: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreAnswer {
    pub restored: u32,
    pub missed: u32,
    pub used: u64,
    pub quota: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfigView {
    pub uploads: bool,
    pub star: bool,
    pub quota: u64,
    pub used: u64,
    pub limits: MediaLimitsView,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaLimitsView {
    pub image_max_bytes: u32,
    pub thumb_max_bytes: u32,
    pub avatar_max_bytes: u32,
    pub album_max: u32,
    pub room_album_max: u32,
    pub restore_window_s: u32,
}

impl From<&MediaConfigView> for MediaConfig {
    fn from(view: &MediaConfigView) -> Self {
        let limits = &view.limits;
        MediaConfig {
            uploads: view.uploads,
            star: view.star,
            quota: view.quota,
            used: view.used,
            limits: MediaLimits {
                image_max_bytes: limits.image_max_bytes,
                thumb_max_bytes: limits.thumb_max_bytes,
                avatar_max_bytes: limits.avatar_max_bytes,
                album_max: limits.album_max,
                room_album_max: limits.room_album_max,
                restore_window_s: limits.restore_window_s,
            },
        }
    }
}
