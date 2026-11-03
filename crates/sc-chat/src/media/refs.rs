use sc_chat_wire::proto::Attachment;
use uuid::Uuid;

use super::ALBUM_MAX;
use crate::contracts::{ChatError, ChatResult};

pub fn post_media(attachments: &[Attachment]) -> ChatResult<Vec<Uuid>> {
    if attachments.len() > ALBUM_MAX {
        return Err(ChatError::Invalid);
    }
    attachments.iter().map(image_id).collect()
}

pub fn image_id(attachment: &Attachment) -> ChatResult<Uuid> {
    Uuid::parse_str(&attachment.media_id).map_err(|_| ChatError::Invalid)
}

pub fn thumb_id(attachment: &Attachment) -> ChatResult<Uuid> {
    let thumb = attachment.thumb.as_ref().ok_or(ChatError::Invalid)?;
    Uuid::parse_str(&thumb.media_id).map_err(|_| ChatError::Invalid)
}
