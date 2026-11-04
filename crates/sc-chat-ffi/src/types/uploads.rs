use std::collections::HashMap;
use std::sync::Arc;

use sc_chat::api::wire::{CommitAnswer, PresignedPut};
use sc_chat::media::queue;
use sc_chat::sc_chat_wire::proto::Attachment;
use sc_chat::store::MediaUpload;
use sc_chat::{ConvId, MediaKind, Millis, UploadObject, UploadState};
use uuid::Uuid;

#[derive(uniffi::Object)]
pub struct MediaSeal {
    pub(crate) kind: MediaKind,
    pub(crate) attachment: Attachment,
}

#[derive(uniffi::Record)]
pub struct PreparedMedia {
    pub kind: MediaKind,
    pub image: Vec<u8>,
    pub thumb: Option<Vec<u8>>,
    pub w: u32,
    pub h: u32,
    pub thumbhash: Vec<u8>,
    pub seal: Arc<MediaSeal>,
}

impl PreparedMedia {
    pub fn new(
        kind: MediaKind,
        image: Vec<u8>,
        thumb: Option<Vec<u8>>,
        attachment: Attachment,
    ) -> Self {
        Self {
            kind,
            image,
            thumb,
            w: attachment.w,
            h: attachment.h,
            thumbhash: attachment.thumbhash.clone(),
            seal: Arc::new(MediaSeal { kind, attachment }),
        }
    }
}

#[derive(uniffi::Record)]
pub struct NewMediaUpload {
    pub album: Uuid,
    pub position: u32,
    pub conversation_id: ConvId,
    pub image_ref: String,
    pub thumb_ref: Option<String>,
    pub seal: Arc<MediaSeal>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaUploadRow {
    pub id: Uuid,
    pub album: Uuid,
    pub position: u32,
    pub conversation_id: ConvId,
    pub kind: MediaKind,
    pub image_ref: String,
    pub thumb_ref: Option<String>,
    pub state: UploadState,
    pub object: UploadObject,
    pub media_id: Option<Uuid>,
    pub thumb_media_id: Option<Uuid>,
    pub stage_id: Option<String>,
    pub parts_sent: u32,
    pub presign_expires_at: Option<Millis>,
    pub image_bytes: u64,
    pub thumb_bytes: u64,
    pub attempts: u32,
    pub created_at: Millis,
    pub updated_at: Millis,
}

impl From<MediaUpload> for MediaUploadRow {
    fn from(upload: MediaUpload) -> Self {
        let thumb = upload.attachment.thumb.as_ref();
        Self {
            thumb_media_id: thumb.and_then(|thumb| Uuid::parse_str(&thumb.media_id).ok()),
            thumb_bytes: thumb.map_or(0, |thumb| thumb.size),
            image_bytes: upload.attachment.size,
            id: upload.id,
            album: upload.album,
            position: upload.position,
            conversation_id: upload.conversation_id,
            kind: upload.kind,
            image_ref: upload.image_ref,
            thumb_ref: upload.thumb_ref,
            state: upload.state,
            object: upload.object,
            media_id: upload.media_id,
            stage_id: upload.stage_id,
            parts_sent: upload.parts_sent,
            presign_expires_at: upload.presign_expires_at,
            attempts: upload.attempts,
            created_at: upload.created_at,
            updated_at: upload.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PutTarget {
    pub url: String,
    pub headers: HashMap<String, String>,
}

impl From<PresignedPut> for PutTarget {
    fn from(put: PresignedPut) -> Self {
        Self {
            url: put.url,
            headers: put.headers.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PutTargets {
    pub expires_at: Millis,
    pub image: PutTarget,
    pub thumb: Option<PutTarget>,
}

impl From<queue::PutTargets> for PutTargets {
    fn from(targets: queue::PutTargets) -> Self {
        Self {
            expires_at: targets.expires_at,
            image: targets.image.into(),
            thumb: targets.thumb.map(PutTarget::from),
        }
    }
}

#[uniffi::remote(Record)]
pub struct CommitAnswer {
    pub id: Uuid,
    pub bytes: u64,
    pub used: u64,
    pub quota: u64,
}
