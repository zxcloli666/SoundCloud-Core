use std::time::Duration;

use sc_chat::media::{MediaError, MediaWorker, open_image, open_thumb};

use crate::support::*;

fn worker() -> MediaWorker {
    MediaWorker::new(2, Duration::from_secs(20))
}

fn patch_vp8_lossy(webp: &mut [u8], w: u16, h: u16) {
    assert_eq!(&webp[12..16], b"VP8 ");
    webp[26..28].copy_from_slice(&w.to_le_bytes());
    webp[28..30].copy_from_slice(&h.to_le_bytes());
}

fn patch_vp8_lossless(webp: &mut [u8], w: u32, h: u32) {
    assert_eq!(&webp[12..16], b"VP8L");
    assert_eq!(webp[20], 0x2f);
    let old = u32::from_le_bytes(webp[21..25].try_into().unwrap());
    let packed = (old & 0xF000_0000) | (w - 1) | ((h - 1) << 14);
    webp[21..25].copy_from_slice(&packed.to_le_bytes());
}

fn lossless_webp(w: u32, h: u32) -> Vec<u8> {
    let image = photo(w, h);
    webp::Encoder::from_rgb(&image, w, h)
        .encode_lossless()
        .to_vec()
}

fn dimensions_refused(plain: &[u8], w: u32, h: u32) {
    let (ciphertext, attachment) = forged(plain, w, h);
    assert_eq!(
        open_image(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Dimensions),
        "{w}x{h}"
    );
}

#[test]
fn a_lossy_header_over_the_caps_is_refused_before_decoding() {
    for (w, h) in [
        (16383, 16383),
        (2049, 16),
        (16, 2049),
        (2048, 2052),
        (4000, 1100),
    ] {
        let mut webp = lossy_webp(64, 64);
        patch_vp8_lossy(&mut webp, w as u16, h as u16);
        dimensions_refused(&webp, w, h);
    }
}

#[test]
fn a_lossless_header_over_the_caps_is_refused_before_decoding() {
    for (w, h) in [(16384, 16384), (3000, 3000), (2049, 1)] {
        let mut webp = lossless_webp(16, 16);
        patch_vp8_lossless(&mut webp, w, h);
        dimensions_refused(&webp, w, h);
    }
}

#[test]
fn the_cap_edge_passes_the_header_check() {
    let mut webp = lossy_webp(64, 64);
    patch_vp8_lossy(&mut webp, 2048, 2048);
    let (ciphertext, attachment) = forged(&webp, 2048, 2048);
    let error = open_image(&worker(), &ciphertext, &attachment).err();
    assert_eq!(error, Some(MediaError::Malformed));
}

#[test]
fn a_thumb_over_512_px_is_refused() {
    let mut webp = lossy_webp(64, 64);
    patch_vp8_lossy(&mut webp, 600, 100);
    let (ciphertext, thumb) = forged_thumb(&webp, 600, 100);
    assert_eq!(
        open_thumb(&worker(), &ciphertext, &thumb).err(),
        Some(MediaError::Dimensions)
    );
}

#[test]
fn a_header_that_disagrees_with_the_attachment_is_refused() {
    let webp = lossy_webp(120, 80);
    let (ciphertext, mut attachment) = forged(&webp, 120, 80);
    attachment.w = 100;
    assert_eq!(
        open_image(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Mismatch)
    );
    attachment.w = 120;
    attachment.h = 81;
    assert_eq!(
        open_image(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Mismatch)
    );
}

#[test]
fn an_animated_webp_is_refused() {
    let frame = photo(40, 30);
    let other = noise(40, 30);
    let config = webp::WebPConfig::new().unwrap();
    let mut animation = webp::AnimEncoder::new(40, 30, &config);
    animation.add_frame(webp::AnimFrame::from_rgb(&frame, 40, 30, 0));
    animation.add_frame(webp::AnimFrame::from_rgb(&other, 40, 30, 100));
    let bytes = animation.encode().to_vec();
    let (ciphertext, attachment) = forged(&bytes, 40, 30);
    assert_eq!(
        open_image(&worker(), &ciphertext, &attachment).err(),
        Some(MediaError::Animated)
    );
}

#[test]
fn garbage_plaintexts_give_typed_errors() {
    let mut zeros_and_noise: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        b"RIFF".to_vec(),
        b"RIFF\xff\xff\xff\xffWEBPVP8 ".to_vec(),
        b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0".to_vec(),
        vec![0; 100_000],
        vec![0xff; 100_000],
    ];
    for size in [12, 30, 64, 1000, 5000] {
        zeros_and_noise.push((0..size).map(|at| hash(at, 7) as u8).collect());
    }
    for plain in zeros_and_noise {
        let (ciphertext, attachment) = forged(&plain, 64, 64);
        assert!(open_image(&worker(), &ciphertext, &attachment).is_err());
    }
}

#[test]
fn truncating_a_real_image_at_every_step_never_panics() {
    let webp = lossy_webp(200, 150);
    let step = (webp.len() / 250).max(1);
    for length in (0..webp.len()).step_by(step) {
        let (ciphertext, attachment) = forged(&webp[..length], 200, 150);
        assert!(open_image(&worker(), &ciphertext, &attachment).is_err());
    }
}

#[test]
fn flipping_bytes_of_a_real_image_never_panics() {
    for source in [lossy_webp(160, 120), lossless_webp(60, 40)] {
        let (w, h) = if source[12..16] == *b"VP8L" {
            (60, 40)
        } else {
            (160, 120)
        };
        for round in 0..300u32 {
            let mut webp = source.clone();
            let at = hash(round, 1) as usize % webp.len();
            webp[at] ^= 1 << (hash(round, 2) % 8);
            let (ciphertext, attachment) = forged(&webp, w, h);
            let _ = open_image(&worker(), &ciphertext, &attachment);
        }
    }
}
