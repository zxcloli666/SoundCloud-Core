use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};
use image_webp::WebPDecoder;
use sc_chat_wire::proto::{Attachment, Thumb};
use sha2::{Digest, Sha256};

use super::cipher::{Role, open_blob};
use super::{
    IMAGE_CAP, MediaError, MediaWorker, RECEIVE_BUFFER_BUDGET, RECEIVE_MAX_PIXELS,
    RECEIVE_MAX_SIDE, RECEIVE_THUMB_SIDE, THUMB_SERVER_CAP,
};

const JPEG_QUALITY: u8 = 88;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub w: u32,
    pub h: u32,
}

struct Claim<'a> {
    role: Role,
    key: &'a [u8],
    nonce: &'a [u8],
    w: u32,
    h: u32,
    sha256_cipher: &'a [u8],
    sha256_plain: &'a [u8],
}

pub fn open_image(
    worker: &MediaWorker,
    ciphertext: &[u8],
    attachment: &Attachment,
) -> Result<OpenedImage, MediaError> {
    open(
        worker,
        ciphertext,
        &Claim {
            role: Role::Image,
            key: &attachment.key,
            nonce: &attachment.nonce,
            w: attachment.w,
            h: attachment.h,
            sha256_cipher: &attachment.sha256_cipher,
            sha256_plain: &attachment.sha256_plain,
        },
    )
}

pub fn open_thumb(
    worker: &MediaWorker,
    ciphertext: &[u8],
    thumb: &Thumb,
) -> Result<OpenedImage, MediaError> {
    open(
        worker,
        ciphertext,
        &Claim {
            role: Role::Thumb,
            key: &thumb.key,
            nonce: &thumb.nonce,
            w: thumb.w,
            h: thumb.h,
            sha256_cipher: &[],
            sha256_plain: &[],
        },
    )
}

fn open(worker: &MediaWorker, ciphertext: &[u8], claim: &Claim) -> Result<OpenedImage, MediaError> {
    let (cap, side_cap) = match claim.role {
        Role::Image => (IMAGE_CAP, RECEIVE_MAX_SIDE),
        Role::Thumb => (THUMB_SERVER_CAP, RECEIVE_THUMB_SIDE),
    };
    if ciphertext.len() > cap {
        return Err(MediaError::TooLarge);
    }
    if !claim.sha256_cipher.is_empty() && Sha256::digest(ciphertext)[..] != *claim.sha256_cipher {
        return Err(MediaError::Mismatch);
    }
    let plain = open_blob(claim.role, claim.key, claim.nonce, ciphertext)?;
    if !claim.sha256_plain.is_empty() && Sha256::digest(&plain[..])[..] != *claim.sha256_plain {
        return Err(MediaError::Mismatch);
    }
    let decoder =
        WebPDecoder::new(Cursor::new(plain.to_vec())).map_err(|_| MediaError::Malformed)?;
    if decoder.is_animated() {
        return Err(MediaError::Animated);
    }
    let (w, h) = decoder.dimensions();
    if w == 0 || h == 0 || w.max(h) > side_cap || u64::from(w) * u64::from(h) > RECEIVE_MAX_PIXELS {
        return Err(MediaError::Dimensions);
    }
    if (w, h) != (claim.w, claim.h) {
        return Err(MediaError::Mismatch);
    }
    let buffer = decoder
        .output_buffer_size()
        .filter(|size| *size <= RECEIVE_BUFFER_BUDGET)
        .ok_or(MediaError::Dimensions)?;
    let alpha = decoder.has_alpha();
    worker.run(move || decode(decoder, buffer, alpha, (w, h)))
}

fn decode(
    mut decoder: WebPDecoder<Cursor<Vec<u8>>>,
    buffer: usize,
    alpha: bool,
    (w, h): (u32, u32),
) -> Result<OpenedImage, MediaError> {
    let mut pixels = vec![0u8; buffer];
    decoder
        .read_image(&mut pixels)
        .map_err(|_| MediaError::Malformed)?;
    let mut bytes = Vec::new();
    match alpha {
        true => {
            PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
                .write_image(&pixels, w, h, ExtendedColorType::Rgba8)
        }
        false => JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY).write_image(
            &pixels,
            w,
            h,
            ExtendedColorType::Rgb8,
        ),
    }
    .map_err(|_| MediaError::Encode)?;
    Ok(OpenedImage {
        bytes,
        mime: if alpha { "image/png" } else { "image/jpeg" },
        w,
        h,
    })
}
