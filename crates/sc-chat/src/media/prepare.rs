use image::DynamicImage;
use image::imageops::FilterType;
use sc_chat_wire::proto::{Attachment, AttachmentKind, Thumb};
use sha2::{Digest, Sha256};

use super::cipher::{Role, seal_blob};
use super::ladder::{self, encode, fit_side};
use super::{IMAGE_SIDE, MIME_WEBP, MediaError, source};

const THUMBHASH_SIDE: u32 = 100;

pub struct PreparedImage {
    pub image: Vec<u8>,
    pub thumb: Vec<u8>,
    pub attachment: Attachment,
}

pub fn prepare_image(bytes: &[u8]) -> Result<PreparedImage, MediaError> {
    let (decoded, profile) = source::decode(bytes)?;
    let scaled = fit_side(&decoded, IMAGE_SIDE);
    drop(decoded);
    let base = match profile {
        Some(profile) => source::to_srgb(scaled, &profile),
        None => scaled,
    };
    let image = encode(&base, &ladder::IMAGE)?;
    let thumb = encode(&base, &ladder::THUMB)?;
    let image_blob = seal_blob(Role::Image, &image.webp)?;
    let thumb_blob = seal_blob(Role::Thumb, &thumb.webp)?;
    let attachment = Attachment {
        media_id: String::new(),
        kind: AttachmentKind::Image as i32,
        mime: MIME_WEBP.into(),
        size: image_blob.ciphertext.len() as u64,
        w: image.w,
        h: image.h,
        thumbhash: thumbhash(&base),
        key: image_blob.key.to_vec(),
        nonce: image_blob.nonce,
        sha256_plain: Sha256::digest(&image.webp).to_vec(),
        sha256_cipher: Sha256::digest(&image_blob.ciphertext).to_vec(),
        thumb: Some(Thumb {
            media_id: String::new(),
            key: thumb_blob.key.to_vec(),
            nonce: thumb_blob.nonce,
            size: thumb_blob.ciphertext.len() as u64,
            w: thumb.w,
            h: thumb.h,
        }),
    };
    Ok(PreparedImage {
        image: image_blob.ciphertext,
        thumb: thumb_blob.ciphertext,
        attachment,
    })
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
