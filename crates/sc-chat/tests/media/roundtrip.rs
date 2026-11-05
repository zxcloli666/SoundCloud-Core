use sc_chat::media::{
    BLOCK, IMAGE_CAP, MediaError, MediaWorker, Role, THUMB_CAP, open_blob, open_image, open_thumb,
    prepare_image,
};
use sha2::{Digest, Sha256};

use crate::support::*;

fn worker() -> MediaWorker {
    MediaWorker::new(2, std::time::Duration::from_secs(60))
}

#[test]
fn a_photo_goes_through_prepare_and_open() {
    let source = jpeg(&photo(2400, 1600));
    let prepared = prepare_image(&source).unwrap();
    let attachment = &prepared.attachment;
    let thumb = attachment.thumb.as_ref().unwrap();
    assert_eq!((attachment.w, attachment.h), (1600, 1067));
    assert!(thumb.w.max(thumb.h) <= 320);
    assert!(prepared.image.len() <= IMAGE_CAP && prepared.image.len().is_multiple_of(BLOCK));
    assert!(prepared.thumb.len() <= THUMB_CAP && prepared.thumb.len().is_multiple_of(BLOCK));
    assert_eq!(attachment.size, prepared.image.len() as u64);
    assert_eq!(thumb.size, prepared.thumb.len() as u64);
    assert_eq!(attachment.mime, "image/webp");
    assert_eq!(attachment.key.len(), 32);
    assert_eq!(attachment.nonce.len(), 24);
    assert!(!attachment.thumbhash.is_empty() && attachment.thumbhash.len() <= 32);
    assert_eq!(
        attachment.sha256_cipher,
        Sha256::digest(&prepared.image).to_vec()
    );

    let worker = worker();
    let full = open_image(&worker, &prepared.image, attachment).unwrap();
    assert_eq!((full.w, full.h, full.mime), (1600, 1067, "image/jpeg"));
    let decoded = image::load_from_memory(&full.bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1600, 1067));
    let small = open_thumb(&worker, &prepared.thumb, thumb).unwrap();
    assert_eq!((small.w, small.h), (thumb.w, thumb.h));
}

#[test]
fn every_file_gets_its_own_key_and_nonce() {
    let source = jpeg(&photo(300, 200));
    let first = prepare_image(&source).unwrap();
    let second = prepare_image(&source).unwrap();
    assert_ne!(first.attachment.key, second.attachment.key);
    assert_ne!(first.attachment.nonce, second.attachment.nonce);
    assert_ne!(first.image, second.image);
    let thumb = first.attachment.thumb.as_ref().unwrap();
    assert_ne!(thumb.key, first.attachment.key);
}

#[test]
fn small_images_are_not_upscaled() {
    let prepared = prepare_image(&jpeg(&photo(50, 40))).unwrap();
    assert_eq!((prepared.attachment.w, prepared.attachment.h), (50, 40));
    let thumb = prepared.attachment.thumb.unwrap();
    assert_eq!((thumb.w, thumb.h), (50, 40));
}

#[test]
fn orientation_is_applied_and_every_tag_is_gone() {
    let mut left_red = image::RgbImage::from_pixel(400, 200, image::Rgb([20, 20, 230]));
    for y in 0..200 {
        for x in 0..200 {
            left_red.put_pixel(x, y, image::Rgb([230, 20, 20]));
        }
    }
    let source = with_exif(&jpeg(&left_red), 6);
    assert!(source.windows(SECRET.len()).any(|part| part == SECRET));
    let prepared = prepare_image(&source).unwrap();
    let attachment = &prepared.attachment;
    assert_eq!((attachment.w, attachment.h), (200, 400));

    let plain = open_blob(
        Role::Image,
        &attachment.key,
        &attachment.nonce,
        &prepared.image,
    )
    .unwrap();
    assert!(!plain.windows(SECRET.len()).any(|part| part == SECRET));
    for chunk in webp_chunks(&plain) {
        assert!(
            ["VP8 ", "VP8X", "ALPH"].contains(&chunk.as_str()),
            "{chunk}"
        );
    }
    let opened = open_image(&worker(), &prepared.image, attachment).unwrap();
    let decoded = image::load_from_memory(&opened.bytes).unwrap().to_rgb8();
    let top = decoded.get_pixel(100, 50);
    let bottom = decoded.get_pixel(100, 350);
    assert!(top[0] > 150 && top[2] < 100, "{top:?}");
    assert!(bottom[2] > 150 && bottom[0] < 100, "{bottom:?}");
}

#[test]
fn incompressible_noise_steps_down_until_it_fits() {
    let prepared = prepare_image(&jpeg(&noise(2000, 1500))).unwrap();
    assert!(prepared.image.len() <= IMAGE_CAP);
    assert!(prepared.attachment.w < 1600);
    let thumb = prepared.attachment.thumb.unwrap();
    assert!(prepared.thumb.len() <= THUMB_CAP && thumb.w <= 320);
}

#[test]
fn transparency_survives_and_opens_as_png() {
    let prepared = prepare_image(&png(&translucent(300, 200))).unwrap();
    let opened = open_image(&worker(), &prepared.image, &prepared.attachment).unwrap();
    assert_eq!(opened.mime, "image/png");
    let decoded = image::load_from_memory(&opened.bytes).unwrap().to_rgba8();
    assert!(decoded.get_pixel(10, 10)[3] < 200);
}

#[test]
fn an_opaque_png_alpha_channel_is_dropped() {
    let mut opaque = translucent(120, 80);
    opaque.pixels_mut().for_each(|pixel| pixel.0[3] = 255);
    let prepared = prepare_image(&png(&opaque)).unwrap();
    let opened = open_image(&worker(), &prepared.image, &prepared.attachment).unwrap();
    assert_eq!(opened.mime, "image/jpeg");
}

#[test]
fn a_gif_gives_its_first_frame() {
    let mut out = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut out);
        let frames = [[255u8, 0, 0, 255], [0, 0, 255, 255]].map(|color| {
            image::Frame::new(image::RgbaImage::from_pixel(64, 48, image::Rgba(color)))
        });
        encoder.encode_frames(frames).unwrap();
    }
    let prepared = prepare_image(&out).unwrap();
    let opened = open_image(&worker(), &prepared.image, &prepared.attachment).unwrap();
    let pixel = image::load_from_memory(&opened.bytes).unwrap().to_rgb8();
    assert!(pixel.get_pixel(5, 5)[0] > 200);
}

#[test]
fn a_webp_source_is_accepted() {
    let prepared = prepare_image(&lossy_webp(500, 300)).unwrap();
    assert_eq!((prepared.attachment.w, prepared.attachment.h), (500, 300));
}

#[test]
fn heic_and_other_formats_are_refused_with_a_type() {
    let mut heic = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic".to_vec();
    heic.extend_from_slice(&[0; 64]);
    assert_eq!(prepare_image(&heic).err(), Some(MediaError::Heic));
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&[0; 64]);
    assert_eq!(prepare_image(&bmp).err(), Some(MediaError::Unsupported));
    assert!(prepare_image(b"").is_err());
    assert!(prepare_image(b"not an image at all").is_err());
}
