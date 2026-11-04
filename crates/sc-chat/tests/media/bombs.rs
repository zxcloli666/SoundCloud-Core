use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use sc_chat::media::{MediaError, prepare_avatar, prepare_backdrop, prepare_image};

use crate::support::*;

type Prepare = fn(&[u8]) -> Result<(), MediaError>;

fn image(bytes: &[u8]) -> Result<(), MediaError> {
    prepare_image(bytes).map(drop)
}

fn avatar(bytes: &[u8]) -> Result<(), MediaError> {
    prepare_avatar(bytes).map(drop)
}

fn backdrop(bytes: &[u8]) -> Result<(), MediaError> {
    prepare_backdrop(bytes).map(drop)
}

const KINDS: [(&str, Prepare); 3] = [("image", image), ("avatar", avatar), ("backdrop", backdrop)];

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

fn huge_gif_screen() -> Vec<u8> {
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
    out
}

#[test]
fn a_png_decompression_bomb_is_refused_on_the_sender_side() {
    let bomb = png_bomb(40_000, 40_000);
    assert!(bomb.len() < 200_000);
    for (name, prepare) in KINDS {
        assert_eq!(prepare(&bomb).err(), Some(MediaError::Dimensions), "{name}");
    }
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
    for (name, prepare) in KINDS {
        assert_eq!(
            prepare(&bytes).err(),
            Some(MediaError::Dimensions),
            "{name}"
        );
    }
}

#[test]
fn a_gif_with_a_huge_screen_never_panics_or_allocates_it() {
    let gif = huge_gif_screen();
    for (_, prepare) in KINDS {
        let _ = prepare(&gif);
    }
}

#[test]
fn an_oversized_file_is_refused_before_decoding() {
    let mut bytes = jpeg(&photo(16, 16));
    bytes.resize(49 * 1024 * 1024, 0);
    for (name, prepare) in KINDS {
        assert_eq!(prepare(&bytes).err(), Some(MediaError::TooLarge), "{name}");
    }
}

#[test]
fn a_truncated_png_or_webp_is_refused_at_every_cut() {
    for source in [png(&translucent(80, 50)), lossy_webp(70, 50)] {
        let step = (source.len() / 60).max(1);
        for length in (0..source.len() - 1).step_by(step) {
            for (name, prepare) in KINDS {
                assert!(prepare(&source[..length]).is_err(), "{name} at {length}");
            }
        }
    }
}

#[test]
fn a_truncated_jpeg_is_refused_or_keeps_what_arrived_without_a_panic() {
    let source = jpeg(&photo(90, 60));
    let step = (source.len() / 60).max(1);
    for length in (0..source.len()).step_by(step) {
        for (_, prepare) in KINDS {
            let _ = prepare(&source[..length]);
        }
    }
    for (name, prepare) in KINDS {
        assert!(prepare(&source[..100]).is_err(), "{name}");
    }
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
            for (_, prepare) in KINDS {
                let _ = prepare(&damaged);
            }
        }
    }
}
