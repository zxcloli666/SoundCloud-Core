use sc_chat::{
    AttachmentKind, AvatarChange, BackdropChange, ConvId, MediaConfig, MediaKind, MediaLimits,
    MediaUploadView, MediaUploadsEvent, MessageAttachment, Millis, UploadObject, UploadState,
};
use uuid::Uuid;

#[uniffi::remote(Enum)]
pub enum AttachmentKind {
    Image,
}

#[uniffi::remote(Record)]
pub struct MessageAttachment {
    pub media_id: Uuid,
    pub kind: AttachmentKind,
    pub size: u64,
    pub w: u32,
    pub h: u32,
    pub thumbhash: Vec<u8>,
}

#[uniffi::remote(Enum)]
pub enum MediaKind {
    Image,
    Avatar,
    Backdrop,
}

#[uniffi::remote(Enum)]
pub enum UploadState {
    Queued,
    Reserved,
    Uploading,
    Uploaded,
    Committed,
    Failed,
}

#[uniffi::remote(Enum)]
pub enum UploadObject {
    Image,
    Thumb,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
pub struct MediaUploadsEvent {
    pub uploads: Vec<MediaUploadView>,
    pub removed: Vec<Uuid>,
}

#[uniffi::remote(Record)]
pub struct MediaLimits {
    pub image_max_bytes: u32,
    pub thumb_max_bytes: u32,
    pub avatar_max_bytes: u32,
    pub album_max: u32,
    pub room_album_max: u32,
    pub restore_window_s: u32,
}

#[uniffi::remote(Record)]
pub struct MediaConfig {
    pub uploads: bool,
    pub star: bool,
    pub quota: u64,
    pub used: u64,
    pub limits: MediaLimits,
}

#[uniffi::remote(Enum)]
pub enum AvatarChange {
    Set { upload_id: Uuid },
    Clear,
}

#[uniffi::remote(Enum)]
pub enum BackdropChange {
    Set { upload_id: Uuid },
    Clear,
}
