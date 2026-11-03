use sc_chat::{
    AttachmentKind, ConvId, MediaKind, MediaUploadView, MediaUploadsEvent, MessageAttachment,
    Millis, UploadObject, UploadState,
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
