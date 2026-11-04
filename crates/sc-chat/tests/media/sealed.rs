use std::time::Duration;

use sc_chat::media::{
    BLOCK, MediaError, MediaWorker, Role, open_blob, open_image, open_thumb, prepare_backdrop,
    prepare_image, seal_blob,
};
use sc_chat::sc_chat_wire::proto::Attachment;

use crate::support::*;

fn worker() -> MediaWorker {
    MediaWorker::new(2, Duration::from_secs(20))
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

#[test]
fn a_real_backdrop_cut_or_flipped_never_opens() {
    let prepared = prepare_backdrop(&jpeg(&photo(2400, 1350))).unwrap();
    let attachment = &prepared.attachment;
    let worker = worker();
    assert!(open_image(&worker, &prepared.image, attachment).is_ok());
    let step = (prepared.image.len() / 40).max(1);
    for length in (0..prepared.image.len()).step_by(step) {
        assert!(open_image(&worker, &prepared.image[..length], attachment).is_err());
    }
    for round in 0..40u32 {
        let mut flipped = prepared.image.clone();
        let at = hash(round, 5) as usize % flipped.len();
        flipped[at] ^= 1 << (hash(round, 6) % 8);
        assert_eq!(
            open_image(&worker, &flipped, attachment).err(),
            Some(MediaError::Mismatch)
        );
    }
}
