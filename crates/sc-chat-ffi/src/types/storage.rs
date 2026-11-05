use sc_chat::api::wire::{
    ConversationUsage, DeleteAnswer, MediaSelection, MediaUrl, MediaUsage, MinePage, MyMedia,
    MyMediaStatus, RestoreAnswer, UrlsAnswer,
};
use sc_chat::media::OpenedImage;
use sc_chat::sc_chat_wire::proto::Attachment;
use sc_chat::{ConvId, MediaKind, MineOrder, MineQuery};
use uuid::Uuid;

#[uniffi::remote(Enum)]
pub enum MediaUrl {
    Ok { id: Uuid, url: String },
    Deleted { id: Uuid },
    Lost { id: Uuid },
}

#[uniffi::remote(Record)]
pub struct UrlsAnswer {
    pub expires_in_s: u32,
    pub items: Vec<MediaUrl>,
}

#[uniffi::remote(Record)]
pub struct ConversationUsage {
    pub conversation_id: Uuid,
    pub bytes: u64,
    pub files: u32,
    pub gone: bool,
}

#[uniffi::remote(Record)]
pub struct MediaUsage {
    pub bytes: u64,
    pub reserved: u64,
    pub quota: u64,
    pub files: u32,
    pub conversations: Vec<ConversationUsage>,
}

#[uniffi::remote(Enum)]
pub enum MineOrder {
    New,
    Old,
    Big,
}

#[uniffi::remote(Record)]
pub struct MineQuery {
    pub order: MineOrder,
    pub conversation_id: Option<ConvId>,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[uniffi::remote(Enum)]
pub enum MyMediaStatus {
    Uploaded,
    Attached,
    Lost,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
pub struct MinePage {
    pub items: Vec<MyMedia>,
    pub next_cursor: Option<String>,
}

#[uniffi::remote(Record)]
pub struct MediaSelection {
    pub ids: Vec<Uuid>,
    pub all: bool,
}

#[uniffi::remote(Record)]
pub struct DeleteAnswer {
    pub deleted: u32,
    pub freed: u64,
    pub used: u64,
    pub quota: u64,
}

#[uniffi::remote(Record)]
pub struct RestoreAnswer {
    pub restored: u32,
    pub missed: u32,
    pub used: u64,
    pub quota: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenedMedia {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub w: u32,
    pub h: u32,
}

impl From<OpenedImage> for OpenedMedia {
    fn from(opened: OpenedImage) -> Self {
        Self {
            bytes: opened.bytes,
            mime: opened.mime.to_owned(),
            w: opened.w,
            h: opened.h,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaThumbInfo {
    pub media_id: Uuid,
    pub size: u64,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaInfo {
    pub media_id: Uuid,
    pub size: u64,
    pub w: u32,
    pub h: u32,
    pub thumbhash: Vec<u8>,
    pub thumb: Option<MediaThumbInfo>,
}

impl MediaInfo {
    pub fn of(attachment: &Attachment) -> Option<Self> {
        let thumb = match &attachment.thumb {
            Some(thumb) => Some(MediaThumbInfo {
                media_id: Uuid::parse_str(&thumb.media_id).ok()?,
                size: thumb.size,
                w: thumb.w,
                h: thumb.h,
            }),
            None => None,
        };
        Some(Self {
            media_id: Uuid::parse_str(&attachment.media_id).ok()?,
            size: attachment.size,
            w: attachment.w,
            h: attachment.h,
            thumbhash: attachment.thumbhash.clone(),
            thumb,
        })
    }
}
