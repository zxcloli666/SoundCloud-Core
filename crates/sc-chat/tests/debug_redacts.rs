use sc_chat::envelope::{self, Content, Post};
use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::{Attachment, AttachmentKind, Thumb};
use sc_chat::store::{StoredContent, StoredMessage};
use sc_chat::{ConversationMeta, MessageClass, RichText};
use uuid::Uuid;

const KEY: u8 = 0xab;
const NONCE: u8 = 0xcd;
const MEDIA: Uuid = Uuid::from_u128(0x61);

fn keyed() -> Attachment {
    Attachment {
        media_id: MEDIA.to_string(),
        kind: AttachmentKind::Image as i32,
        mime: "image/webp".into(),
        size: 4096,
        w: 640,
        h: 480,
        thumbhash: vec![KEY; 8],
        key: vec![KEY; 32],
        nonce: vec![NONCE; 24],
        sha256_plain: vec![KEY; 32],
        sha256_cipher: vec![NONCE; 32],
        thumb: Some(Thumb {
            media_id: Uuid::from_u128(0x62).to_string(),
            key: vec![KEY; 32],
            nonce: vec![NONCE; 24],
            size: 512,
            w: 320,
            h: 240,
        }),
    }
}

fn post() -> Content {
    Content::Post(Post {
        text: RichText {
            text: "кадр".into(),
            spans: Vec::new(),
        },
        reply_to: None,
        mentions: Vec::new(),
        embeds: Vec::new(),
        attachments: vec![keyed()],
        forwarded_from: None,
    })
}

fn no_bytes(printed: &str) {
    for byte in [KEY, NONCE] {
        assert!(!printed.contains(&format!("{byte}, {byte}")), "{printed}");
    }
}

fn no_key(printed: &str) {
    no_bytes(printed);
    assert!(printed.contains(&MEDIA.to_string()), "{printed}");
}

#[test]
fn an_attachment_prints_its_ids_and_byte_lengths_only() {
    let printed = format!("{:?}", keyed());
    no_key(&printed);
    assert!(printed.contains("key: <32 bytes>"), "{printed}");
    assert!(printed.contains("nonce: <24 bytes>"), "{printed}");
    assert!(
        printed.contains(&Uuid::from_u128(0x62).to_string()),
        "{printed}"
    );
}

#[test]
fn metas_posts_and_contents_print_no_media_key() {
    let meta = ConversationMeta {
        avatar: Some(keyed()),
        ..ConversationMeta::default()
    };
    no_key(&format!("{meta:?}"));
    no_key(&format!("{:?}", Content::Meta(Box::new(meta))));
    let Content::Post(body) = post() else {
        unreachable!()
    };
    no_key(&format!("{body:?}"));
    no_key(&format!("{:?}", post()));
}

#[test]
fn sealed_envelope_bytes_print_as_a_length() {
    let bytes = envelope::encode(&post()).unwrap().encode_to_vec();
    assert!(bytes.windows(32).any(|part| part == [KEY; 32]));
    let item = OutboxItem {
        client_id: Uuid::from_u128(0x1),
        conversation_id: Uuid::from_u128(0x2),
        class: MessageClass::Post,
        target_seq: None,
        envelope: bytes.clone(),
        created_at: 1,
        attempts: 0,
        next_at: 1,
    };
    let printed = format!("{item:?}");
    assert!(
        printed.contains(&format!("<{} bytes>", bytes.len())),
        "{printed}"
    );
    no_bytes(&printed);

    let stored = StoredMessage {
        conversation_id: Uuid::from_u128(0x2),
        seq: 3,
        client_id: Uuid::from_u128(0x1),
        sender: "100".into(),
        sender_device: None,
        class: MessageClass::Post,
        epoch: 1,
        target_seq: None,
        ts: 1,
        expires_at: None,
        local_only: false,
        content: StoredContent::Open(bytes.clone()),
    };
    let printed = format!("{stored:?}");
    assert!(
        printed.contains(&format!("Open(<{} bytes>)", bytes.len())),
        "{printed}"
    );
    no_bytes(&printed);
    let sealed = format!("{:?}", StoredContent::Sealed(bytes.clone()));
    assert_eq!(sealed, format!("Sealed(<{} bytes>)", bytes.len()));
}
