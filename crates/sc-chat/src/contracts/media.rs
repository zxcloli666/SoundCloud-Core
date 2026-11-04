use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ids::{ConvId, Millis};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Avatar,
    Backdrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageAttachment {
    pub media_id: Uuid,
    pub kind: AttachmentKind,
    pub size: u64,
    pub w: u32,
    pub h: u32,
    pub thumbhash: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AvatarChange {
    Set { upload_id: Uuid },
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum BackdropChange {
    Set { upload_id: Uuid },
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadState {
    Queued,
    Reserved,
    Uploading,
    Uploaded,
    Committed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadObject {
    Image,
    Thumb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaUploadView {
    pub id: Uuid,
    pub album: Uuid,
    pub position: u32,
    pub conversation_id: ConvId,
    pub kind: MediaKind,
    pub state: UploadState,
    pub object: UploadObject,
    pub parts_sent: u32,
    pub image_bytes: u64,
    pub thumb_bytes: u64,
    pub attempts: u32,
    pub updated_at: Millis,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaUploadsEvent {
    pub uploads: Vec<MediaUploadView>,
    pub removed: Vec<Uuid>,
}
