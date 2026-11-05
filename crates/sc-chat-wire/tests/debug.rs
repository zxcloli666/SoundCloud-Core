use sc_chat_wire::proto::{Attachment, Envelope, Thumb};

#[test]
fn media_keys_never_reach_debug_output() {
    let thumb = Thumb {
        media_id: "thumb-id".into(),
        key: vec![0xab; 32],
        nonce: vec![0xcd; 24],
        ..Thumb::default()
    };
    let attachment = Attachment {
        media_id: "image-id".into(),
        key: vec![0xab; 32],
        nonce: vec![0xcd; 24],
        sha256_plain: vec![0xab; 32],
        thumb: Some(thumb),
        ..Attachment::default()
    };
    let envelope = Envelope {
        attachments: vec![attachment],
        ..Envelope::default()
    };
    let printed = format!("{envelope:?}");
    assert!(!printed.contains("171, 171"), "{printed}");
    assert!(!printed.contains("205, 205"), "{printed}");
    assert!(
        printed.contains("image-id") && printed.contains("thumb-id"),
        "{printed}"
    );
    assert!(printed.contains("key: <32 bytes>"), "{printed}");
}
