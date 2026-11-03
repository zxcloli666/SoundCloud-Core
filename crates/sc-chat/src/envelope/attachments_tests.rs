use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::{self, Attachment, AttachmentKind, Envelope, Thumb, envelope::Body};
use uuid::Uuid;

use super::{Content, Post, decode, encode};
use crate::contracts::{ChatError, ConversationMeta, RichText};
use crate::media::{ALBUM_MAX, AVATAR_SERVER_CAP};

fn image(n: u128) -> Attachment {
    Attachment {
        media_id: Uuid::from_u128(n).to_string(),
        kind: AttachmentKind::Image as i32,
        mime: "image/webp".into(),
        size: 1_000,
        w: 640,
        h: 480,
        thumbhash: vec![1, 2, 3],
        key: vec![7; 32],
        nonce: vec![8; 24],
        sha256_plain: vec![1; 32],
        sha256_cipher: vec![2; 32],
        thumb: Some(Thumb {
            media_id: Uuid::from_u128(n + 1_000).to_string(),
            key: vec![9; 32],
            nonce: vec![6; 24],
            size: 100,
            w: 320,
            h: 240,
        }),
    }
}

fn avatar(n: u128) -> Attachment {
    Attachment {
        thumb: None,
        ..image(n)
    }
}

fn album(attachments: Vec<Attachment>, text: &str) -> Post {
    Post {
        text: RichText {
            text: text.into(),
            spans: Vec::new(),
        },
        reply_to: None,
        mentions: Vec::new(),
        embeds: Vec::new(),
        attachments,
        forwarded_from: None,
    }
}

fn round_trip(content: &Content) -> Content {
    let bytes = encode(content).unwrap().encode_to_vec();
    decode(&Envelope::decode(bytes.as_slice()).unwrap()).unwrap()
}

#[test]
fn an_album_with_or_without_text_survives_a_round_trip() {
    for text in ["", "вот фотки"] {
        let post = Content::Post(album(vec![image(1), image(2)], text));
        assert_eq!(round_trip(&post), post);
    }
}

#[test]
fn a_bad_album_is_refused_when_sending() {
    let mut no_thumb = image(1);
    no_thumb.thumb = None;
    let mut short_key = image(2);
    short_key.key.truncate(16);
    let mut bad_id = image(3);
    bad_id.media_id = "m1".into();
    let too_many = (0..=ALBUM_MAX as u128).map(image).collect();
    for attachments in [vec![no_thumb], vec![short_key], vec![bad_id], too_many] {
        assert_eq!(
            encode(&Content::Post(album(attachments, "x"))).err(),
            Some(ChatError::Invalid)
        );
    }
}

#[test]
fn bad_attachments_from_others_are_dropped_and_the_album_is_capped() {
    let mut no_thumb = image(1);
    no_thumb.thumb = None;
    let mut attachments = vec![no_thumb];
    attachments.extend((10..10 + ALBUM_MAX as u128 + 2).map(image));
    let envelope = Envelope {
        v: 1,
        body: Some(Body::Text(proto::Text {
            text: String::new(),
            spans: Vec::new(),
        })),
        attachments,
        ..Envelope::default()
    };
    let Content::Post(post) = decode(&envelope).unwrap() else {
        panic!("a text body decodes as a post");
    };
    let expected: Vec<_> = (10..10 + ALBUM_MAX as u128).map(image).collect();
    assert_eq!(post.attachments, expected);
}

#[test]
fn a_group_avatar_rides_the_meta_envelope() {
    let meta = ConversationMeta {
        title: "Ночная смена".into(),
        description: String::new(),
        avatar: Some(avatar(5)),
    };
    let envelope = encode(&Content::Meta(meta.clone())).unwrap();
    let Some(Body::Meta(wire)) = &envelope.body else {
        panic!("meta encodes as a meta body");
    };
    assert_eq!(wire.avatar_media_id, Uuid::from_u128(5).to_string());
    assert_eq!(envelope.attachments, vec![avatar(5)]);
    assert_eq!(
        round_trip(&Content::Meta(meta.clone())),
        Content::Meta(meta)
    );
}

#[test]
fn an_avatar_with_a_thumb_or_too_big_is_refused() {
    let mut big = avatar(5);
    big.size = AVATAR_SERVER_CAP as u64 + 1;
    for bad in [image(5), big] {
        let meta = ConversationMeta {
            title: "t".into(),
            description: String::new(),
            avatar: Some(bad),
        };
        assert_eq!(encode(&Content::Meta(meta)).err(), Some(ChatError::Invalid));
    }
}

#[test]
fn a_meta_avatar_id_without_its_attachment_reads_as_no_avatar() {
    let envelope = Envelope {
        v: 1,
        body: Some(Body::Meta(proto::Meta {
            title: "t".into(),
            description: String::new(),
            avatar_media_id: Uuid::from_u128(5).to_string(),
        })),
        attachments: vec![avatar(6), image(5)],
        ..Envelope::default()
    };
    let Content::Meta(meta) = decode(&envelope).unwrap() else {
        panic!("a meta body decodes as meta");
    };
    assert_eq!(meta.avatar, None);
}
