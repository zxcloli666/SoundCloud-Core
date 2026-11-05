use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage, Rgba, RgbaImage};
use sc_chat::media::{Role, seal_blob};
use sc_chat::sc_chat_wire::proto::{Attachment, Thumb};
use sc_chat::{CoreEvent, Events, MediaUploadsEvent};
use std::sync::Mutex;

#[derive(Default)]
pub struct Seen(Mutex<Vec<MediaUploadsEvent>>);

impl Events for Seen {
    fn emit(&self, event: CoreEvent) {
        if let CoreEvent::MediaUploads(uploads) = event {
            self.0.lock().unwrap().push(uploads);
        }
    }
}

impl Seen {
    pub fn take(&self) -> Vec<MediaUploadsEvent> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

pub const SECRET: &[u8] = b"SECRET-GPS-48.8566N";

pub fn hash(x: u32, y: u32) -> u32 {
    let mut v = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA6B);
    v ^= v >> 15;
    v = v.wrapping_mul(0x2C1B_3C6D);
    v ^ (v >> 13)
}

pub fn photo(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let grain = (hash(x, y) % 14) as f32;
        let r = 120.0 + 90.0 * (fx * 7.0 + fy * 2.0).sin() + grain;
        let g = 110.0 + 80.0 * (fy * 9.0 - fx).cos() + grain;
        let b = 100.0 + 70.0 * ((fx + fy) * 5.0).sin() + grain;
        Rgb([r as u8, g as u8, b as u8])
    })
}

pub fn noise(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let v = hash(x, y);
        Rgb([v as u8, (v >> 8) as u8, (v >> 16) as u8])
    })
}

pub fn translucent(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([200, (x % 256) as u8, (y % 256) as u8, 90])
    })
}

pub fn jpeg(image: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(
            image,
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

pub fn png(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            image,
            image.width(),
            image.height(),
            ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

pub fn with_exif(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut tiff = b"MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01".to_vec();
    tiff.extend_from_slice(&orientation.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    tiff.extend_from_slice(SECRET);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend(tiff);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&(app1.len() as u16 + 2).to_be_bytes());
    out.extend(app1);
    out.extend_from_slice(&[0xFF, 0xFE]);
    out.extend_from_slice(&(SECRET.len() as u16 + 2).to_be_bytes());
    out.extend_from_slice(SECRET);
    out.extend_from_slice(&jpeg[2..]);
    out
}

pub fn webp_chunks(webp: &[u8]) -> Vec<String> {
    assert_eq!(&webp[..4], b"RIFF");
    assert_eq!(&webp[8..12], b"WEBP");
    let mut at = 12;
    let mut found = Vec::new();
    while at + 8 <= webp.len() {
        found.push(String::from_utf8_lossy(&webp[at..at + 4]).into_owned());
        let size = u32::from_le_bytes(webp[at + 4..at + 8].try_into().unwrap()) as usize;
        at += 8 + size + (size & 1);
    }
    found
}

pub fn lossy_webp(w: u32, h: u32) -> Vec<u8> {
    let image = photo(w, h);
    webp::Encoder::from_rgb(&image, w, h).encode(50.0).to_vec()
}

pub fn forged(plain: &[u8], w: u32, h: u32) -> (Vec<u8>, Attachment) {
    let blob = seal_blob(Role::Image, plain).unwrap();
    let attachment = Attachment {
        w,
        h,
        key: blob.key.to_vec(),
        nonce: blob.nonce,
        size: blob.ciphertext.len() as u64,
        ..Default::default()
    };
    (blob.ciphertext, attachment)
}

pub fn forged_thumb(plain: &[u8], w: u32, h: u32) -> (Vec<u8>, Thumb) {
    let blob = seal_blob(Role::Thumb, plain).unwrap();
    let thumb = Thumb {
        w,
        h,
        key: blob.key.to_vec(),
        nonce: blob.nonce,
        size: blob.ciphertext.len() as u64,
        ..Default::default()
    };
    (blob.ciphertext, thumb)
}
