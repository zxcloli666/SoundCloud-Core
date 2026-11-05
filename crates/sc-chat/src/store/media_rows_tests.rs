use sc_chat_wire::proto::{Attachment, Thumb};
use uuid::Uuid;

use super::NewUpload;
use crate::contracts::MediaKind;
use crate::store::Store;

const KEY: u8 = 0xab;

fn keyed() -> NewUpload {
    NewUpload {
        album: Uuid::from_u128(1),
        position: 0,
        conversation_id: Uuid::from_u128(2),
        kind: MediaKind::Image,
        image_ref: "media/a.img".into(),
        thumb_ref: Some("media/a.thumb".into()),
        attachment: Attachment {
            media_id: Uuid::from_u128(3).to_string(),
            size: 4096,
            key: vec![KEY; 32],
            nonce: vec![KEY; 24],
            sha256_plain: vec![KEY; 32],
            thumbhash: vec![KEY; 8],
            thumb: Some(Thumb {
                media_id: Uuid::from_u128(4).to_string(),
                key: vec![KEY; 32],
                nonce: vec![KEY; 24],
                size: 512,
                ..Thumb::default()
            }),
            ..Attachment::default()
        },
    }
}

fn shows_no_key(text: &str) {
    assert!(!text.contains(&format!("{KEY}")), "{text}");
}

#[test]
fn an_upload_prints_its_ids_and_never_its_keys() {
    let upload = keyed();
    let printed = format!("{upload:?}");
    shows_no_key(&printed);
    assert!(
        printed.contains(&Uuid::from_u128(3).to_string()),
        "{printed}"
    );
    assert!(
        printed.contains(&Uuid::from_u128(4).to_string()),
        "{printed}"
    );

    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = store.enqueue_upload(&upload, 1).unwrap();
    let stored = store.upload(id).unwrap().unwrap();
    assert_eq!(stored.attachment.key, vec![KEY; 32]);
    let printed = format!("{stored:?}");
    shows_no_key(&printed);
    assert!(printed.contains(&id.to_string()), "{printed}");
}
