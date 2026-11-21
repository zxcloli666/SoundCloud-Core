use std::io::Cursor;

use image::codecs::gif::GifEncoder;
use image::{Delay, Frame, Rgba, RgbaImage};
use image_webp::WebPDecoder;
use sc_chat::media::{
    IMAGE_CAP, MOTION_DURATION_MS, MOTION_FRAMES, MOTION_SIDE, MediaError, MediaWorker, Role,
    THUMB_CAP, open_image, open_motion, open_thumb, prepare_image, seal_blob,
};
use sc_chat::sc_chat_wire::proto::{Attachment, Motion};
use sc_chat::store::{Reserved, Store, UploadObject, UploadState};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::queue::{IMAGE_ID, MASTER, upload};
use crate::support::*;

const MOTION_ID: Uuid = Uuid::from_u128(0x1c);

fn worker() -> MediaWorker {
    MediaWorker::new(2, std::time::Duration::from_secs(60))
}

pub fn scene(w: u32, h: u32, step: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let ball = (x + w - step * 7 % w) % w < w / 5 && (y + step * 3) % h < h / 4;
        match ball {
            true => Rgba([250, 200, 40, 255]),
            false => Rgba([(x * 255 / w) as u8, (y * 255 / h) as u8, 120, 255]),
        }
    })
}

pub fn gif(w: u32, h: u32, delays: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut out, 30);
        let frames = delays.iter().enumerate().map(|(step, ms)| {
            Frame::from_parts(
                scene(w, h, step as u32),
                0,
                0,
                Delay::from_numer_denom_ms(*ms, 1),
            )
        });
        encoder.encode_frames(frames).unwrap();
    }
    out
}

fn durations(webp: &[u8]) -> Vec<u32> {
    let mut decoder = WebPDecoder::new(Cursor::new(webp)).unwrap();
    let mut canvas = vec![0u8; decoder.output_buffer_size().unwrap()];
    (0..decoder.num_frames())
        .map(|_| decoder.read_frame(&mut canvas).unwrap())
        .collect()
}

fn animated(w: u32, h: u32, frames: u32, ms: i32) -> Vec<u8> {
    let config = webp::WebPConfig::new().unwrap();
    let images: Vec<RgbaImage> = (0..frames)
        .map(|n| RgbaImage::from_pixel(w, h, Rgba([n as u8, (n * 7) as u8, 90, 255])))
        .collect();
    let mut animation = webp::AnimEncoder::new(w, h, &config);
    for (n, image) in images.iter().enumerate() {
        animation.add_frame(webp::AnimFrame::from_rgba(image, w, h, n as i32 * ms));
    }
    animation.encode().to_vec()
}

fn forged_motion(plain: &[u8]) -> (Vec<u8>, Attachment) {
    let decoder = WebPDecoder::new(Cursor::new(plain)).unwrap();
    let (w, h) = decoder.dimensions();
    let frames = decoder.num_frames();
    let duration_ms = decoder.loop_duration() as u32;
    let blob = seal_blob(Role::Image, plain).unwrap();
    let motion = Motion {
        key: blob.key.to_vec(),
        nonce: blob.nonce,
        size: blob.ciphertext.len() as u64,
        w,
        h,
        sha256_plain: Sha256::digest(plain).to_vec(),
        sha256_cipher: Sha256::digest(&blob.ciphertext).to_vec(),
        frames,
        duration_ms,
        ..Motion::default()
    };
    let attachment = Attachment {
        motion: Some(motion),
        ..Attachment::default()
    };
    (blob.ciphertext, attachment)
}

fn opened(plain: &[u8]) -> Result<(), MediaError> {
    let (ciphertext, attachment) = forged_motion(plain);
    open_motion(&worker(), &ciphertext, &attachment).map(|_| ())
}

#[test]
fn a_gif_keeps_a_still_image_and_carries_its_animation() {
    let delays = [80, 120, 200, 60, 500, 90];
    let source = gif(200, 150, &delays);
    let prepared = prepare_image(&source).unwrap();
    let attachment = &prepared.attachment;
    let motion = attachment.motion.as_ref().unwrap();
    assert_eq!((attachment.w, attachment.h), (200, 150));
    assert_eq!((motion.w, motion.h), (200, 150));
    assert_eq!(motion.frames, delays.len() as u32);
    assert_eq!(motion.duration_ms, delays.iter().sum::<u32>());
    let sealed = prepared.motion.as_ref().unwrap();
    assert!(sealed.len() <= IMAGE_CAP);
    assert_eq!(motion.size, sealed.len() as u64);
    assert_eq!(motion.sha256_cipher, Sha256::digest(sealed).to_vec());
    assert_ne!(motion.key, attachment.key);

    let worker = worker();
    let still = open_image(&worker, &prepared.image, attachment).unwrap();
    assert_eq!((still.w, still.h, still.mime), (200, 150, "image/jpeg"));
    let thumb = attachment.thumb.as_ref().unwrap();
    assert!(prepared.thumb.len() <= THUMB_CAP);
    open_thumb(&worker, &prepared.thumb, thumb).unwrap();

    let played = open_motion(&worker, sealed, attachment).unwrap();
    assert_eq!((played.w, played.h, played.mime), (200, 150, "image/webp"));
    assert_eq!(durations(&played.bytes), delays);
}

#[test]
fn a_still_image_has_no_motion() {
    let photo = prepare_image(&jpeg(&photo(640, 480))).unwrap();
    assert!(photo.motion.is_none() && photo.attachment.motion.is_none());
    let one_frame = prepare_image(&gif(64, 48, &[100])).unwrap();
    assert!(one_frame.motion.is_none() && one_frame.attachment.motion.is_none());
    assert_eq!(
        open_motion(&worker(), &photo.image, &photo.attachment).err(),
        Some(MediaError::Mismatch)
    );
}

#[test]
fn a_big_gif_shrinks_to_the_motion_side() {
    let prepared = prepare_image(&gif(800, 600, &[100, 100, 100])).unwrap();
    let motion = prepared.attachment.motion.as_ref().unwrap();
    assert_eq!((motion.w, motion.h), (MOTION_SIDE, 360));
    assert_eq!((prepared.attachment.w, prepared.attachment.h), (800, 600));
}

#[test]
fn many_frames_merge_and_keep_the_timing() {
    let delays = vec![40; 240];
    let prepared = prepare_image(&gif(48, 36, &delays)).unwrap();
    let motion = prepared.attachment.motion.as_ref().unwrap();
    assert!(motion.frames <= MOTION_FRAMES);
    assert_eq!(motion.duration_ms, 240 * 40);
}

#[test]
fn a_long_gif_stops_at_ten_seconds() {
    let prepared = prepare_image(&gif(48, 36, &[3_000; 5])).unwrap();
    let motion = prepared.attachment.motion.as_ref().unwrap();
    assert_eq!((motion.frames, motion.duration_ms), (3, 9_000));
    assert!(motion.duration_ms <= MOTION_DURATION_MS);
}

#[test]
fn too_many_frames_are_refused() {
    assert_eq!(opened(&animated(16, 16, 151, 40)), Err(MediaError::Motion));
    assert_eq!(opened(&animated(16, 16, 150, 40)), Ok(()));
}

#[test]
fn too_long_an_animation_is_refused() {
    assert_eq!(opened(&animated(16, 16, 2, 6_000)), Err(MediaError::Motion));
    assert_eq!(opened(&animated(16, 16, 2, 5_000)), Ok(()));
}

#[test]
fn too_many_pixels_across_frames_are_refused() {
    assert_eq!(
        opened(&animated(1000, 1000, 40, 100)),
        Err(MediaError::Dimensions)
    );
    assert_eq!(opened(&animated(1000, 1000, 30, 100)), Ok(()));
}

#[test]
fn a_motion_that_disagrees_with_its_claim_is_refused() {
    let (ciphertext, mut attachment) = forged_motion(&animated(16, 16, 4, 100));
    attachment.motion.as_mut().unwrap().frames = 3;
    assert_eq!(
        open_motion(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Mismatch)
    );
}

#[test]
fn a_still_webp_is_not_a_motion() {
    let (ciphertext, mut attachment) = forged(&lossy_webp(40, 30), 40, 30);
    attachment.motion = Some(Motion {
        key: attachment.key.clone(),
        nonce: attachment.nonce.clone(),
        w: 40,
        h: 30,
        ..Motion::default()
    });
    assert_eq!(
        open_motion(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Malformed)
    );
}

#[test]
fn an_upload_with_motion_walks_image_thumb_motion() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let prepared = prepare_image(&gif(64, 48, &[100, 100])).unwrap();
    let mut new = upload(Uuid::new_v4(), 0);
    new.attachment = prepared.attachment;
    new.motion_ref = Some("media/a/0.motion".into());
    let id = store.enqueue_upload(&new, 100).unwrap();
    let reserved = Reserved {
        media_id: IMAGE_ID,
        thumb_media_id: Some(Uuid::from_u128(0x1b)),
        motion_media_id: Some(MOTION_ID),
        presign_expires_at: 10_000,
    };
    store.upload_reserved(id, &reserved, 200).unwrap();
    let row = store.upload(id).unwrap().unwrap();
    assert_eq!(
        row.attachment.motion.as_ref().unwrap().media_id,
        MOTION_ID.to_string()
    );
    let mut walked = Vec::new();
    for object in [
        UploadObject::Image,
        UploadObject::Thumb,
        UploadObject::Motion,
    ] {
        store.upload_object_done(id, object, 300).unwrap();
        let row = store.upload(id).unwrap().unwrap();
        walked.push((row.state, row.object));
    }
    assert_eq!(
        walked,
        [
            (UploadState::Uploading, UploadObject::Thumb),
            (UploadState::Uploading, UploadObject::Motion),
            (UploadState::Uploaded, UploadObject::Motion),
        ]
    );
    let mut lost = upload(Uuid::new_v4(), 1);
    lost.motion_ref = Some("media/a/1.motion".into());
    assert!(store.enqueue_upload(&lost, 100).is_err());
}
