use sc_chat::crypto::{
    BODY_MAX, Ck, META_MAX, MessageAt, open_message, open_meta, seal_message, seal_meta,
};
use sc_chat::sc_chat_wire::msg::{MessageAad, PAD_BLOCK};
use sc_chat::sc_chat_wire::proto::{Envelope, Meta, Pin, Reaction, Text, envelope::Body};
use sc_chat::sc_chat_wire::seal::NONCE_LEN;
use sc_chat::{ChatError, MessageClass};
use uuid::Uuid;

const TAG_LEN: usize = 16;

fn text(text: &str) -> Envelope {
    Envelope {
        v: 1,
        body: Some(Body::Text(Text {
            text: text.into(),
            spans: Vec::new(),
        })),
        ..Envelope::default()
    }
}

fn at() -> MessageAt {
    MessageAt {
        conversation_id: Uuid::new_v4(),
        epoch: 3,
        sender_device: Uuid::new_v4(),
        client_id: Uuid::new_v4(),
        class: MessageClass::Post,
    }
}

#[test]
fn a_message_opens_only_where_it_was_sealed() {
    let ck = Ck::from_bytes([4; 32]);
    let at = at();
    let body = seal_message(&ck, &at, &text("привет")).unwrap();
    assert_eq!(
        open_message(&ck, &at, &body).unwrap().body,
        text("привет").body
    );
    let moved = [
        MessageAt {
            class: MessageClass::Edit,
            ..at
        },
        MessageAt { epoch: 4, ..at },
        MessageAt {
            conversation_id: Uuid::new_v4(),
            ..at
        },
        MessageAt {
            sender_device: Uuid::new_v4(),
            ..at
        },
        MessageAt {
            client_id: Uuid::new_v4(),
            ..at
        },
    ];
    for other in moved {
        assert_eq!(
            open_message(&ck, &other, &body).err(),
            Some(ChatError::Crypto),
            "{other:?}"
        );
    }
    let other_ck = Ck::from_bytes([5; 32]);
    assert_eq!(
        open_message(&other_ck, &at, &body).err(),
        Some(ChatError::Crypto)
    );
}

#[test]
fn bodies_are_padded_to_whole_blocks_and_never_repeat() {
    let ck = Ck::from_bytes([4; 32]);
    let at = at();
    for size in [0, 1, 200, 253, 254, 255, 256, 1000, 4096] {
        let body = seal_message(&ck, &at, &text(&"x".repeat(size))).unwrap();
        assert_eq!(
            (body.len() - NONCE_LEN - TAG_LEN) % PAD_BLOCK,
            0,
            "size {size}"
        );
    }
    let first = seal_message(&ck, &at, &text("same")).unwrap();
    let second = seal_message(&ck, &at, &text("same")).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.len(), second.len());
}

#[test]
fn system_rows_and_oversized_bodies_are_never_sealed() {
    let ck = Ck::from_bytes([4; 32]);
    let system = MessageAt {
        class: MessageClass::System,
        ..at()
    };
    assert_eq!(
        seal_message(&ck, &system, &text("hi")).err(),
        Some(ChatError::Invalid)
    );
    assert_eq!(
        open_message(&ck, &system, b"{}").err(),
        Some(ChatError::Invalid)
    );
    let huge = Envelope {
        mentions: vec!["1".repeat(BODY_MAX)],
        ..text("hi")
    };
    assert_eq!(
        seal_message(&ck, &at(), &huge).err(),
        Some(ChatError::Invalid)
    );
    let big_meta = Envelope {
        mentions: vec!["1".repeat(META_MAX)],
        ..Envelope::default()
    };
    assert_eq!(
        seal_meta(&ck, Uuid::new_v4(), 1, &big_meta).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn group_meta_is_bound_to_its_conversation_and_epoch_only() {
    let ck = Ck::from_bytes([4; 32]);
    let conv = Uuid::new_v4();
    let meta = Envelope {
        v: 1,
        body: Some(Body::Meta(Meta {
            title: "Ночная смена".into(),
            ..Meta::default()
        })),
        ..Envelope::default()
    };
    let meta_ct = seal_meta(&ck, conv, 2, &meta).unwrap();
    assert_eq!(open_meta(&ck, conv, 2, &meta_ct).unwrap().body, meta.body);
    assert_eq!(
        open_meta(&ck, conv, 3, &meta_ct).err(),
        Some(ChatError::Crypto)
    );
    let from_any_device = MessageAt {
        conversation_id: conv,
        epoch: 2,
        class: MessageClass::Meta,
        ..at()
    };
    let message = seal_message(&ck, &from_any_device, &meta).unwrap();
    assert_eq!(open_meta(&ck, conv, 2, &message).unwrap().body, meta.body);
    let as_post = MessageAt {
        class: MessageClass::Post,
        ..from_any_device
    };
    assert_eq!(
        open_message(&ck, &as_post, &message).err(),
        Some(ChatError::Crypto)
    );
}

#[test]
fn a_garbled_body_does_not_open() {
    let ck = Ck::from_bytes([4; 32]);
    let at = at();
    let mut body = seal_message(&ck, &at, &text("hi")).unwrap();
    let last = body.len() - 1;
    body[last] ^= 1;
    assert_eq!(open_message(&ck, &at, &body).err(), Some(ChatError::Crypto));
    assert_eq!(
        open_message(&ck, &at, &body[..10]).err(),
        Some(ChatError::Crypto)
    );
}

fn forged(ck: &Ck, at: &MessageAt, envelope: &Envelope) -> Vec<u8> {
    let aad = MessageAad {
        conv: *at.conversation_id.as_bytes(),
        epoch: at.epoch,
        sender_device: *at.sender_device.as_bytes(),
        client_id: *at.client_id.as_bytes(),
        class: at.class.wire(),
    };
    let nonce = [7; NONCE_LEN];
    sc_chat::sc_chat_wire::msg::seal(ck.as_bytes(), &aad, nonce, envelope).unwrap()
}

fn carrying(body: Body) -> Envelope {
    Envelope {
        v: 1,
        body: Some(body),
        ..Envelope::default()
    }
}

#[test]
fn a_body_never_opens_under_a_class_it_does_not_belong_to() {
    let ck = Ck::from_bytes([4; 32]);
    let pin = carrying(Body::Pin(Pin {
        target: 9,
        on: true,
    }));
    let reaction = carrying(Body::Reaction(Reaction {
        target: 9,
        emoji: "🔥".into(),
        on: true,
    }));
    let cases = [
        (MessageClass::React, pin.clone()),
        (MessageClass::Post, pin),
        (MessageClass::Pin, reaction),
        (MessageClass::Edit, text("hi")),
    ];
    for (class, envelope) in cases {
        let at = MessageAt { class, ..at() };
        assert_eq!(
            seal_message(&ck, &at, &envelope).err(),
            Some(ChatError::Invalid),
            "{class:?}"
        );
        let body = forged(&ck, &at, &envelope);
        assert_eq!(
            open_message(&ck, &at, &body).err(),
            Some(ChatError::Protocol),
            "{class:?}"
        );
    }
    let future = MessageAt {
        class: MessageClass::React,
        ..at()
    };
    let body = forged(&ck, &future, &Envelope::default());
    assert_eq!(open_message(&ck, &future, &body).unwrap().body, None);
}
