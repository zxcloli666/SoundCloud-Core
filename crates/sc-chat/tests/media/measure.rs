use std::time::Instant;

use sc_chat::media::{MediaWorker, open_image, open_thumb, prepare_image};

use crate::support::*;

#[test]
#[ignore]
fn twelve_megapixels() {
    let base = photo(4000, 3000);
    let detailed = image::RgbImage::from_fn(4000, 3000, |x, y| {
        let wave = ((x as f32 * 0.37).sin() * (y as f32 * 0.29).cos() * 40.0) as i32;
        let grain = (hash(x, y) % 40) as i32 - 20;
        let pixel = base.get_pixel(x, y);
        image::Rgb(
            pixel
                .0
                .map(|c| (c as i32 + wave + grain).clamp(0, 255) as u8),
        )
    });
    let source = jpeg(&detailed);
    println!("source jpeg {} bytes", source.len());
    for round in 0..3 {
        let started = Instant::now();
        let prepared = prepare_image(&source).unwrap();
        let prepare = started.elapsed();
        let worker = MediaWorker::default();
        let started = Instant::now();
        let opened = open_image(&worker, &prepared.image, &prepared.attachment).unwrap();
        let open = started.elapsed();
        let thumb = prepared.attachment.thumb.as_ref().unwrap();
        let started = Instant::now();
        open_thumb(&worker, &prepared.thumb, thumb).unwrap();
        let open_small = started.elapsed();
        println!(
            "round {round}: prepare {prepare:?}, image {} bytes ({}x{}), thumb {} bytes ({}x{}), open {open:?} -> {} bytes, open thumb {open_small:?}",
            prepared.image.len(),
            prepared.attachment.w,
            prepared.attachment.h,
            prepared.thumb.len(),
            thumb.w,
            thumb.h,
            opened.bytes.len(),
        );
    }
}
