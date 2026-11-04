use std::time::Duration;

use sc_chat::MediaKind;
use sc_chat::media::{BACKDROP_SERVER_CAP, BLOCK, MediaWorker, open_image, prepare_backdrop};
use sc_chat::store::{NewUpload, Store, UploadObject, UploadState};
use std::sync::Arc;
use uuid::Uuid;

use crate::support::*;

fn worker() -> MediaWorker {
    MediaWorker::new(2, Duration::from_secs(60))
}

fn opened_pixel(bytes: &[u8]) -> image::Rgb<u8> {
    let prepared = prepare_backdrop(bytes).unwrap();
    let opened = open_image(&worker(), &prepared.image, &prepared.attachment).unwrap();
    *image::load_from_memory(&opened.bytes)
        .unwrap()
        .to_rgb8()
        .get_pixel(5, 5)
}

#[test]
fn a_backdrop_is_one_sealed_1920_image_without_a_thumb() {
    let prepared = prepare_backdrop(&jpeg(&photo(2400, 1600))).unwrap();
    let attachment = &prepared.attachment;
    assert!(attachment.thumb.is_none());
    assert_eq!((attachment.w, attachment.h), (1920, 1280));
    assert!(prepared.image.len() <= BACKDROP_SERVER_CAP);
    assert!(prepared.image.len().is_multiple_of(BLOCK));
    assert_eq!(attachment.size, prepared.image.len() as u64);
    assert_eq!(attachment.mime, "image/webp");
    assert!(!attachment.thumbhash.is_empty() && attachment.thumbhash.len() <= 32);
    let opened = open_image(&worker(), &prepared.image, attachment).unwrap();
    assert_eq!((opened.w, opened.h), (1920, 1280));
}

#[test]
fn a_portrait_backdrop_keeps_1920_on_its_long_side() {
    let prepared = prepare_backdrop(&jpeg(&photo(1200, 2400))).unwrap();
    assert_eq!((prepared.attachment.w, prepared.attachment.h), (960, 1920));
}

#[test]
fn a_small_backdrop_is_not_upscaled() {
    let prepared = prepare_backdrop(&jpeg(&photo(800, 450))).unwrap();
    assert_eq!((prepared.attachment.w, prepared.attachment.h), (800, 450));
}

#[test]
fn incompressible_noise_steps_down_until_it_fits() {
    let prepared = prepare_backdrop(&png(
        &image::DynamicImage::ImageRgb8(noise(2000, 1500)).into_rgba8()
    ))
    .unwrap();
    let side = prepared.attachment.w.max(prepared.attachment.h);
    assert!(side < 1920, "{side}");
    assert!(prepared.image.len() <= BACKDROP_SERVER_CAP);
    assert!(open_image(&worker(), &prepared.image, &prepared.attachment).is_ok());
}

#[test]
fn an_animated_gif_backdrop_keeps_its_first_frame_only() {
    let mut out = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut out);
        let frames = [[255u8, 0, 0, 255], [0, 0, 255, 255]].map(|color| {
            image::Frame::new(image::RgbaImage::from_pixel(64, 48, image::Rgba(color)))
        });
        encoder.encode_frames(frames).unwrap();
    }
    let pixel = opened_pixel(&out);
    assert!(pixel[0] > 200 && pixel[2] < 60, "{pixel:?}");
}

#[test]
fn an_animated_webp_backdrop_keeps_its_first_frame_only() {
    let red = image::RgbImage::from_pixel(40, 30, image::Rgb([250, 10, 10]));
    let blue = image::RgbImage::from_pixel(40, 30, image::Rgb([10, 10, 250]));
    let config = webp::WebPConfig::new().unwrap();
    let mut animation = webp::AnimEncoder::new(40, 30, &config);
    animation.add_frame(webp::AnimFrame::from_rgb(&red, 40, 30, 0));
    animation.add_frame(webp::AnimFrame::from_rgb(&blue, 40, 30, 100));
    let bytes = animation.encode().to_vec();
    let pixel = opened_pixel(&bytes);
    assert!(pixel[0] > 200 && pixel[2] < 60, "{pixel:?}");
}

#[test]
fn a_backdrop_upload_is_one_object_from_queue_to_commit() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let seen = Arc::new(Seen::default());
    store.watch_uploads(Some(seen.clone()));
    let prepared = prepare_backdrop(&jpeg(&photo(2000, 1200))).unwrap();
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: Uuid::from_u128(0x60),
        kind: MediaKind::Backdrop,
        image_ref: "media/backdrop.img".into(),
        thumb_ref: None,
        attachment: prepared.attachment.clone(),
    };
    let id = store.enqueue_upload(&upload, 1).unwrap();
    store
        .upload_reserved(
            id,
            &sc_chat::store::Reserved {
                media_id: Uuid::from_u128(0x61),
                thumb_media_id: None,
                presign_expires_at: 10_000,
            },
            2,
        )
        .unwrap();
    store
        .upload_progress(id, UploadObject::Image, 2, Some("stage"), 3)
        .unwrap();
    store
        .upload_object_done(id, UploadObject::Image, 4)
        .unwrap();
    store.upload_committed(id, 5).unwrap();

    let done = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (done.kind, done.state, done.object, done.thumb_ref),
        (
            MediaKind::Backdrop,
            UploadState::Committed,
            UploadObject::Image,
            None
        )
    );
    assert_eq!(done.attachment.media_id, Uuid::from_u128(0x61).to_string());
    assert_eq!(done.attachment.key, prepared.attachment.key);
    let views: Vec<_> = seen
        .take()
        .into_iter()
        .flat_map(|event| event.uploads)
        .map(|view| (view.kind, view.state, view.thumb_bytes))
        .collect();
    assert_eq!(
        views,
        vec![
            (MediaKind::Backdrop, UploadState::Queued, 0),
            (MediaKind::Backdrop, UploadState::Reserved, 0),
            (MediaKind::Backdrop, UploadState::Uploading, 0),
            (MediaKind::Backdrop, UploadState::Uploaded, 0),
            (MediaKind::Backdrop, UploadState::Committed, 0),
        ]
    );
}
