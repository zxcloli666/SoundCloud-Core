use std::io::Cursor;

use image_webp::WebPDecoder;
use sc_chat_wire::proto::Attachment;

use super::cipher::Role;
use super::open::{Claim, unseal};
use super::{
    MIME_WEBP, MOTION_DURATION_MS, MOTION_FRAMES, MOTION_PIXELS, MediaError, MediaWorker,
    OpenedImage, RECEIVE_BUFFER_BUDGET, RECEIVE_MAX_PIXELS, RECEIVE_MAX_SIDE,
};

pub fn open_motion(
    worker: &MediaWorker,
    ciphertext: &[u8],
    attachment: &Attachment,
) -> Result<OpenedImage, MediaError> {
    let motion = attachment.motion.as_ref().ok_or(MediaError::Mismatch)?;
    let plain = unseal(
        ciphertext,
        &Claim {
            role: Role::Image,
            key: &motion.key,
            nonce: &motion.nonce,
            w: motion.w,
            h: motion.h,
            sha256_cipher: &motion.sha256_cipher,
            sha256_plain: &motion.sha256_plain,
        },
    )?;
    let bytes = plain.to_vec();
    let decoder =
        WebPDecoder::new(Cursor::new(plain.to_vec())).map_err(|_| MediaError::Malformed)?;
    if !decoder.is_animated() {
        return Err(MediaError::Malformed);
    }
    let (w, h) = decoder.dimensions();
    let pixels = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || w.max(h) > RECEIVE_MAX_SIDE || pixels > RECEIVE_MAX_PIXELS {
        return Err(MediaError::Dimensions);
    }
    let frames = decoder.num_frames();
    if frames > MOTION_FRAMES || decoder.loop_duration() > u64::from(MOTION_DURATION_MS) {
        return Err(MediaError::Motion);
    }
    if pixels * u64::from(frames) > MOTION_PIXELS {
        return Err(MediaError::Dimensions);
    }
    if (w, h, frames) != (motion.w, motion.h, motion.frames)
        || decoder.loop_duration() != u64::from(motion.duration_ms)
    {
        return Err(MediaError::Mismatch);
    }
    let buffer = decoder
        .output_buffer_size()
        .filter(|size| *size <= RECEIVE_BUFFER_BUDGET)
        .ok_or(MediaError::Dimensions)?;
    worker.run(move || play(decoder, buffer, frames, bytes))
}

fn play(
    mut decoder: WebPDecoder<Cursor<Vec<u8>>>,
    buffer: usize,
    frames: u32,
    bytes: Vec<u8>,
) -> Result<OpenedImage, MediaError> {
    let (w, h) = decoder.dimensions();
    let mut canvas = vec![0u8; buffer];
    for _ in 0..frames {
        decoder
            .read_frame(&mut canvas)
            .map_err(|_| MediaError::Malformed)?;
    }
    Ok(OpenedImage {
        bytes,
        mime: MIME_WEBP,
        w,
        h,
    })
}
