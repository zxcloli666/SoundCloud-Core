use image::DynamicImage;
use image::imageops::FilterType;
use sc_chat_wire::proto::{Attachment, AttachmentKind, Thumb};
use sha2::{Digest, Sha256};

use super::cipher::{Blob, Role, seal_blob};
use super::ladder::{self, Encoded, encode, fit_side};
use super::{AVATAR_SIDE, IMAGE_SIDE, MIME_WEBP, MediaError, source};

const THUMBHASH_SIDE: u32 = 100;

pub struct PreparedImage {
    pub image: Vec<u8>,
    pub thumb: Vec<u8>,
    pub attachment: Attachment,
}

pub struct PreparedAvatar {
    pub image: Vec<u8>,
    pub attachment: Attachment,
}

pub fn prepare_image(bytes: &[u8]) -> Result<PreparedImage, MediaError> {
    let base = base(bytes, IMAGE_SIDE)?;
    let image = encode(&base, &ladder::IMAGE)?;
    let thumb = encode(&base, &ladder::THUMB)?;
    let image_blob = seal_blob(Role::Image, &image.webp)?;
    let thumb_blob = seal_blob(Role::Thumb, &thumb.webp)?;
    let thumb = Thumb {
        media_id: String::new(),
        key: thumb_blob.key.to_vec(),
        nonce: thumb_blob.nonce,
        size: thumb_blob.ciphertext.len() as u64,
        w: thumb.w,
        h: thumb.h,
    };
    Ok(PreparedImage {
        attachment: attachment(&base, &image, &image_blob, Some(thumb)),
        image: image_blob.ciphertext,
        thumb: thumb_blob.ciphertext,
    })
}

pub fn prepare_avatar(bytes: &[u8]) -> Result<PreparedAvatar, MediaError> {
    let base = base(bytes, AVATAR_SIDE)?;
    let image = encode(&base, &ladder::AVATAR)?;
    let image_blob = seal_blob(Role::Image, &image.webp)?;
    Ok(PreparedAvatar {
        attachment: attachment(&base, &image, &image_blob, None),
        image: image_blob.ciphertext,
    })
}

fn base(bytes: &[u8], side: u32) -> Result<DynamicImage, MediaError> {
    let (decoded, profile) = source::decode(bytes)?;
    let scaled = fit_side(&decoded, side);
    drop(decoded);
    Ok(match profile {
        Some(profile) => source::to_srgb(scaled, &profile),
        None => scaled,
    })
}

fn attachment(
    base: &DynamicImage,
    image: &Encoded,
    blob: &Blob,
    thumb: Option<Thumb>,
) -> Attachment {
    Attachment {
        media_id: String::new(),
        kind: AttachmentKind::Image as i32,
        mime: MIME_WEBP.into(),
        size: blob.ciphertext.len() as u64,
        w: image.w,
        h: image.h,
        thumbhash: thumbhash(base),
        key: blob.key.to_vec(),
        nonce: blob.nonce.clone(),
        sha256_plain: Sha256::digest(&image.webp).to_vec(),
        sha256_cipher: Sha256::digest(&blob.ciphertext).to_vec(),
        thumb,
    }
}

fn thumbhash(base: &DynamicImage) -> Vec<u8> {
    let small = base
        .resize(THUMBHASH_SIDE, THUMBHASH_SIDE, FilterType::Triangle)
        .into_rgba8();
    thumbhash::rgba_to_thumb_hash(
        small.width() as usize,
        small.height() as usize,
        small.as_raw(),
    )
}
