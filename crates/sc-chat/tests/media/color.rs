use sc_chat::media::{MediaWorker, Role, open_blob, open_image, prepare_image};

use crate::support::*;

const P3: [u8; 3] = [220, 90, 70];

fn center(source: &[u8]) -> [u8; 3] {
    let prepared = prepare_image(source).unwrap();
    let opened = open_image(
        &MediaWorker::default(),
        &prepared.image,
        &prepared.attachment,
    )
    .unwrap();
    let decoded = image::load_from_memory(&opened.bytes).unwrap().to_rgb8();
    decoded
        .get_pixel(decoded.width() / 2, decoded.height() / 2)
        .0
}

fn near(found: [u8; 3], expected: [u8; 3]) {
    for at in 0..3 {
        assert!(
            found[at].abs_diff(expected[at]) <= 6,
            "{found:?} vs {expected:?}"
        );
    }
}

#[test]
fn a_p3_jpeg_lands_on_the_srgb_value() {
    let source = with_icc_jpeg(&jpeg(&flat(64, 64, P3)), &p3_profile());
    let expected = p3_to_srgb(P3);
    assert!(expected[0].abs_diff(P3[0]) > 12);
    near(center(&source), expected);
}

#[test]
fn a_p3_png_lands_on_the_srgb_value() {
    let source = with_icc_png(
        &png(&image::DynamicImage::ImageRgb8(flat(64, 64, P3)).to_rgba8()),
        &p3_profile(),
    );
    near(center(&source), p3_to_srgb(P3));
}

#[test]
fn a_p3_webp_lands_on_the_srgb_value() {
    let plain = webp::Encoder::from_rgb(&flat(64, 64, P3), 64, 64)
        .encode(90.0)
        .to_vec();
    near(
        center(&with_icc_webp(&plain, &p3_profile(), 64, 64)),
        p3_to_srgb(P3),
    );
}

#[test]
fn a_source_without_a_profile_is_taken_as_srgb() {
    near(center(&jpeg(&flat(64, 64, P3))), P3);
}

#[test]
fn a_malformed_or_cut_profile_falls_back_to_srgb() {
    let base = jpeg(&flat(64, 64, P3));
    let garbage: Vec<u8> = (0..400).map(|at| hash(at, 9) as u8).collect();
    let cut = p3_profile()[..300].to_vec();
    let mut lying = p3_profile();
    lying[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    for profile in [garbage, cut, lying, vec![0; 128], Vec::new()] {
        near(center(&with_icc_jpeg(&base, &profile)), P3);
    }
}

#[test]
fn the_output_stays_untagged() {
    let source = with_icc_jpeg(&jpeg(&flat(64, 64, P3)), &p3_profile());
    let prepared = prepare_image(&source).unwrap();
    let attachment = &prepared.attachment;
    let plain = open_blob(
        Role::Image,
        &attachment.key,
        &attachment.nonce,
        &prepared.image,
    )
    .unwrap();
    for chunk in webp_chunks(&plain) {
        assert!(
            ["VP8 ", "VP8X", "ALPH"].contains(&chunk.as_str()),
            "{chunk}"
        );
    }
}
