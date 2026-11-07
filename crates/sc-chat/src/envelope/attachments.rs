use sc_chat_wire::proto::{Attachment, AttachmentKind as WireKind, Thumb};
use uuid::Uuid;

use crate::contracts::{AttachmentKind, ConversationMeta, MessageAttachment};
use crate::media::{
    ALBUM_MAX, AVATAR_SERVER_CAP, BACKDROP_SERVER_CAP, IMAGE_CAP, THUMB_SERVER_CAP,
};

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SHA256_LEN: usize = 32;

pub fn album(attachments: &[Attachment]) -> Vec<Attachment> {
    attachments
        .iter()
        .filter(|attachment| image_ok(attachment))
        .take(ALBUM_MAX)
        .cloned()
        .collect()
}

pub fn meta_image(attachments: &[Attachment], media_id: &str) -> Option<Attachment> {
    attachments
        .iter()
        .find(|attachment| attachment.media_id == media_id)
        .filter(|attachment| meta_image_ok(attachment))
        .cloned()
}

pub fn view(attachment: &Attachment) -> Option<MessageAttachment> {
    Some(MessageAttachment {
        media_id: Uuid::parse_str(&attachment.media_id).ok()?,
        kind: AttachmentKind::Image,
        size: attachment.size,
        w: attachment.w,
        h: attachment.h,
        thumbhash: attachment.thumbhash.clone(),
    })
}

pub fn image_ok(attachment: &Attachment) -> bool {
    sealed_ok(attachment, IMAGE_CAP) && attachment.thumb.as_ref().is_some_and(thumb_ok)
}

pub fn meta_image_ok(attachment: &Attachment) -> bool {
    sealed_ok(attachment, BACKDROP_SERVER_CAP) && attachment.thumb.is_none()
}

pub fn group_meta(mut meta: ConversationMeta) -> ConversationMeta {
    meta.avatar = meta
        .avatar
        .filter(|avatar| sealed_ok(avatar, AVATAR_SERVER_CAP));
    meta.backdrop = meta.backdrop.filter(meta_image_ok);
    meta
}

fn sealed_ok(attachment: &Attachment, cap: usize) -> bool {
    attachment.kind == WireKind::Image as i32
        && id_ok(&attachment.media_id)
        && keyed(&attachment.key, &attachment.nonce)
        && size_ok(attachment.size, cap)
        && attachment.w > 0
        && attachment.h > 0
        && attachment.sha256_cipher.len() == SHA256_LEN
}

fn thumb_ok(thumb: &Thumb) -> bool {
    id_ok(&thumb.media_id)
        && keyed(&thumb.key, &thumb.nonce)
        && size_ok(thumb.size, THUMB_SERVER_CAP)
        && thumb.w > 0
        && thumb.h > 0
}

fn id_ok(id: &str) -> bool {
    Uuid::parse_str(id).is_ok()
}

fn keyed(key: &[u8], nonce: &[u8]) -> bool {
    key.len() == KEY_LEN && nonce.len() == NONCE_LEN
}

fn size_ok(size: u64, cap: usize) -> bool {
    size > 0 && usize::try_from(size).is_ok_and(|size| size <= cap)
}
