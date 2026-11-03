use std::io::Write;
use std::time::Duration;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use sc_chat::media::{
    BLOCK, MediaError, MediaWorker, Role, open_blob, open_image, open_thumb, prepare_image,
    seal_blob,
};
use sc_chat::sc_chat_wire::proto::Attachment;

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

#[test]
fn a_tampered_ciphertext_never_opens() {
    let prepared = prepare_image(&jpeg(&photo(120, 90))).unwrap();
    let worker = worker();
    let attachment = &prepared.attachment;

    let mut flipped = prepared.image.clone();
    flipped[10] ^= 1;
    let mut loose = attachment.clone();
    loose.sha256_cipher.clear();
    assert_eq!(
        open_image(&worker, &flipped, &loose).err(),
        Some(MediaError::Cipher)
    );
    assert_eq!(
        open_image(&worker, &flipped, attachment).err(),
        Some(MediaError::Mismatch)
    );

    let mut wrong_key = attachment.clone();
    wrong_key.key[0] ^= 1;
    assert_eq!(
        open_image(&worker, &prepared.image, &wrong_key).err(),
        Some(MediaError::Cipher)
    );

    let mut wrong_nonce = attachment.clone();
    wrong_nonce.nonce[0] ^= 1;
    assert_eq!(
        open_image(&worker, &prepared.image, &wrong_nonce).err(),
        Some(MediaError::Cipher)
    );

    let mut short_key = attachment.clone();
    short_key.key.truncate(16);
    assert_eq!(
        open_image(&worker, &prepared.image, &short_key).err(),
        Some(MediaError::Cipher)
    );

    let mut wrong_plain = attachment.clone();
    wrong_plain.sha256_plain[0] ^= 1;
    assert_eq!(
        open_image(&worker, &prepared.image, &wrong_plain).err(),
        Some(MediaError::Mismatch)
    );

    let mut unpadded = prepared.image.clone();
    unpadded.truncate(prepared.image.len() - 1);
    assert_eq!(
        open_image(&worker, &unpadded, &loose).err(),
        Some(MediaError::Cipher)
    );
    assert_eq!(
        open_image(&worker, &[], &loose).err(),
        Some(MediaError::Cipher)
    );
}

#[test]
fn the_image_role_does_not_open_as_a_thumb() {
    let prepared = prepare_image(&jpeg(&photo(120, 90))).unwrap();
    let attachment = &prepared.attachment;
    let as_thumb = sc_chat::sc_chat_wire::proto::Thumb {
        key: attachment.key.clone(),
        nonce: attachment.nonce.clone(),
        w: attachment.w,
        h: attachment.h,
        ..Default::default()
    };
    assert_eq!(
        open_thumb(&worker(), &prepared.image, &as_thumb).err(),
        Some(MediaError::Cipher)
    );
}

#[test]
fn oversized_ciphertexts_are_refused_before_any_work() {
    let big = vec![0u8; 700 * 1024];
    let attachment = Attachment {
        key: vec![1; 32],
        nonce: vec![2; 24],
        ..Default::default()
    };
    assert_eq!(
        open_image(&worker(), &big, &attachment).err(),
        Some(MediaError::TooLarge)
    );
    let thumb = sc_chat::sc_chat_wire::proto::Thumb {
        key: vec![1; 32],
        nonce: vec![2; 24],
        ..Default::default()
    };
    assert_eq!(
        open_thumb(&worker(), &big[..40 * 1024], &thumb).err(),
        Some(MediaError::TooLarge)
    );
}

#[test]
fn padding_hides_the_length_in_4k_steps() {
    let small = seal_blob(Role::Image, &[7; 10]).unwrap();
    let nearly = seal_blob(Role::Image, &vec![7; BLOCK - 20]).unwrap();
    let over = seal_blob(Role::Image, &vec![7; BLOCK - 16]).unwrap();
    assert_eq!(small.ciphertext.len(), BLOCK);
    assert_eq!(nearly.ciphertext.len(), BLOCK);
    assert_eq!(over.ciphertext.len(), 2 * BLOCK);
    let back = open_blob(Role::Image, &*small.key, &small.nonce, &small.ciphertext).unwrap();
    assert_eq!(&back[..], &[7; 10]);
}

fn png_bomb(w: u32, h: u32) -> Vec<u8> {
    let chunk = |kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32fast::hash(&body).to_be_bytes());
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&w.to_be_bytes());
    header.extend_from_slice(&h.to_be_bytes());
    header.extend_from_slice(&[1, 0, 0, 0, 0]);
    chunk(b"IHDR", &header, &mut out);
    let row = vec![0u8; (w as usize).div_ceil(8) + 1];
    let mut zlib = ZlibEncoder::new(Vec::new(), Compression::best());
    for _ in 0..h.min(4000) {
        zlib.write_all(&row).unwrap();
    }
    chunk(b"IDAT", &zlib.finish().unwrap(), &mut out);
    chunk(b"IEND", &[], &mut out);
    out
}

#[test]
fn a_png_decompression_bomb_is_refused_on_the_sender_side() {
    let bomb = png_bomb(40_000, 40_000);
    assert!(bomb.len() < 200_000);
    assert_eq!(prepare_image(&bomb).err(), Some(MediaError::Dimensions));
}

#[test]
fn a_jpeg_with_a_huge_frame_header_is_refused() {
    let mut bytes = jpeg(&photo(32, 32));
    let sof = bytes
        .windows(2)
        .position(|pair| pair == [0xFF, 0xC0])
        .unwrap();
    bytes[sof + 5..sof + 7].copy_from_slice(&60_000u16.to_be_bytes());
    bytes[sof + 7..sof + 9].copy_from_slice(&60_000u16.to_be_bytes());
    assert_eq!(prepare_image(&bytes).err(), Some(MediaError::Dimensions));
}

#[test]
fn a_gif_with_a_huge_screen_never_panics_or_allocates_it() {
    let mut out = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut out);
        let frame = image::Frame::new(image::RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([1, 2, 3, 255]),
        ));
        encoder.encode_frame(frame).unwrap();
    }
    out[6..8].copy_from_slice(&65_535u16.to_le_bytes());
    out[8..10].copy_from_slice(&65_535u16.to_le_bytes());
    let _ = prepare_image(&out);
}

#[test]
fn an_oversized_file_is_refused_before_decoding() {
    let mut bytes = jpeg(&photo(16, 16));
    bytes.resize(49 * 1024 * 1024, 0);
    assert_eq!(prepare_image(&bytes).err(), Some(MediaError::TooLarge));
}

#[test]
fn damaged_sources_never_panic() {
    let sources = [
        jpeg(&photo(90, 60)),
        png(&translucent(80, 50)),
        lossy_webp(70, 50),
    ];
    for source in sources {
        for round in 0..150u32 {
            let mut damaged = source.clone();
            let at = hash(round, 3) as usize % damaged.len();
            damaged[at] ^= 1 << (hash(round, 4) % 8);
            if round % 5 == 0 {
                damaged.truncate(at.max(1));
            }
            let _ = prepare_image(&damaged);
        }
    }
}
