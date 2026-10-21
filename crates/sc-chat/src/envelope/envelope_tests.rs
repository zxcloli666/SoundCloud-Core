use sc_chat_wire::msg::MessageAad;
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::{self, Envelope, envelope::Body};
use uuid::Uuid;

use super::{Content, Post, decode, encode, open, seal};
use crate::contracts::{
    ChatError, ConversationMeta, Embed, EmbedKind, EmbedTrack, Forwarded, MessageClass, RichText,
    RoomInvite, Span, SpanKind,
};
use crate::crypto::{Ck, MessageAt, open_meta, seal_message};

fn track(urn: &str) -> Embed {
    Embed {
        kind: EmbedKind::Track,
        urn: urn.into(),
        permalink: "https://soundcloud.com/a/b".into(),
        secret_token: None,
        title: "Night Drive".into(),
        artist: Some("Artist".into()),
        artwork_url: Some("https://i1.sndcdn.com/artworks-1-t500x500.jpg".into()),
        duration_ms: Some(214_000),
        access: Some("playable".into()),
        at_ms: None,
        tracks: Vec::new(),
        room: None,
    }
}

fn span(kind: SpanKind, start: u32, len: u32) -> Span {
    Span {
        kind,
        start,
        len,
        url: None,
        user_id: None,
    }
}

fn plain(text: &str) -> RichText {
    RichText {
        text: text.into(),
        spans: Vec::new(),
    }
}

fn post(text: RichText) -> Post {
    Post {
        text,
        reply_to: None,
        mentions: Vec::new(),
        embeds: Vec::new(),
        forwarded_from: None,
    }
}

fn round_trip(content: &Content) -> Content {
    let bytes = encode(content).unwrap().encode_to_vec();
    decode(&Envelope::decode(bytes.as_slice()).unwrap()).unwrap()
}

#[test]
fn every_kind_of_content_survives_a_round_trip() {
    let mut playlist = track("soundcloud:playlists:7");
    playlist.kind = EmbedKind::Playlist;
    playlist.tracks = vec![EmbedTrack {
        urn: "soundcloud:tracks:8".into(),
        title: "One".into(),
        artist: "Two".into(),
        duration_ms: 1_000,
        artwork_url: None,
    }];
    let mut invite = track("soundcloud:rooms:abc");
    invite.kind = EmbedKind::RoomInvite;
    invite.room = Some(RoomInvite {
        code: "ABC123".into(),
        title: None,
        host: "lena".into(),
    });
    let rich = RichText {
        text: "привет @lena и ссылка".into(),
        spans: vec![
            Span {
                user_id: Some("42".into()),
                ..span(SpanKind::Mention, 7, 5)
            },
            Span {
                url: Some("https://soundcloud.com".into()),
                ..span(SpanKind::Link, 15, 6)
            },
            span(SpanKind::Spoiler, 0, 6),
        ],
    };
    let contents = [
        Content::Post(Post {
            text: rich.clone(),
            reply_to: Some(5),
            mentions: vec!["42".into()],
            embeds: vec![track("soundcloud:tracks:1"), playlist, invite],
            forwarded_from: Some(Forwarded {
                user_id: "9".into(),
                name: "Max".into(),
                sent_at_ms: 1_760_958_000_000,
            }),
        }),
        Content::Edit {
            target: 3,
            text: rich,
        },
        Content::Delete { target: 4 },
        Content::Reaction {
            target: 5,
            emoji: "👍🏽".into(),
            on: false,
        },
        Content::Meta(ConversationMeta {
            title: "Ночная смена".into(),
            description: String::new(),
            avatar_media_id: Some("m1".into()),
        }),
        Content::Pin {
            target: 6,
            on: true,
        },
        Content::Suggest(track("soundcloud:tracks:2")),
    ];
    for content in contents {
        assert_eq!(round_trip(&content), content);
    }
}

#[test]
fn a_body_this_build_does_not_know_is_unknown() {
    let mut future = Envelope {
        v: 1,
        ..Envelope::default()
    }
    .encode_to_vec();
    future.extend_from_slice(&[0x4a, 0x02, 0x08, 0x01]);
    let decoded = Envelope::decode(future.as_slice()).unwrap();
    assert_eq!(decode(&decoded).unwrap(), Content::Unknown);
    assert_eq!(encode(&Content::Unknown).err(), Some(ChatError::Invalid));
}

#[test]
fn text_is_capped_in_utf16_units() {
    let fits = Content::Post(post(plain(&"😀".repeat(2048))));
    assert_eq!(round_trip(&fits), fits);
    let over = Content::Post(post(plain(&"😀".repeat(2049))));
    assert_eq!(encode(&over).err(), Some(ChatError::Invalid));
    let wire = Envelope {
        body: Some(Body::Text(proto::Text {
            text: "a".repeat(4097),
            spans: Vec::new(),
        })),
        ..Envelope::default()
    };
    assert_eq!(decode(&wire).err(), Some(ChatError::Protocol));
}

fn wire_span(span: Span) -> proto::Span {
    let kind = match span.kind {
        SpanKind::Bold => proto::SpanKind::Bold,
        SpanKind::Mention => proto::SpanKind::Mention,
        SpanKind::Link => proto::SpanKind::Link,
        _ => proto::SpanKind::Unspecified,
    };
    proto::Span {
        kind: kind.into(),
        start: span.start,
        len: span.len,
        url: span.url.unwrap_or_default(),
        user_id: span.user_id.unwrap_or_default(),
    }
}

#[test]
fn bad_spans_are_refused_when_sending_and_dropped_when_reading() {
    let bad = [
        span(SpanKind::Bold, 3, 2),
        span(SpanKind::Bold, 0, 0),
        span(SpanKind::Mention, 0, 2),
        Span {
            url: Some("javascript:alert(1)".into()),
            ..span(SpanKind::Link, 0, 2)
        },
        Span {
            url: Some("https://a b".into()),
            ..span(SpanKind::Link, 0, 2)
        },
    ];
    for span in bad {
        let text = RichText {
            text: "hey".into(),
            spans: vec![span.clone()],
        };
        assert_eq!(
            encode(&Content::Post(post(text))).err(),
            Some(ChatError::Invalid)
        );
        let wire = Envelope {
            body: Some(Body::Text(proto::Text {
                text: "hey".into(),
                spans: vec![wire_span(span)],
            })),
            ..Envelope::default()
        };
        let Content::Post(read) = decode(&wire).unwrap() else {
            panic!("a text body is a post");
        };
        assert!(read.text.spans.is_empty());
    }
}

#[test]
fn a_span_keeps_only_the_field_of_its_kind() {
    let text = RichText {
        text: "hey".into(),
        spans: vec![Span {
            url: Some("https://soundcloud.com".into()),
            user_id: Some("1".into()),
            ..span(SpanKind::Bold, 0, 3)
        }],
    };
    let Content::Post(read) = round_trip(&Content::Post(post(text))) else {
        panic!("a post stays a post");
    };
    assert_eq!(read.text.spans, vec![span(SpanKind::Bold, 0, 3)]);
}

#[test]
fn embeds_are_capped_at_three() {
    let mut four = post(plain("look"));
    four.embeds = (0..4)
        .map(|n| track(&format!("soundcloud:tracks:{n}")))
        .collect();
    assert_eq!(
        encode(&Content::Post(four.clone())).err(),
        Some(ChatError::Invalid)
    );
    let mut wire = encode(&Content::Post(post(plain("look")))).unwrap();
    wire.embeds = four
        .embeds
        .iter()
        .map(super::snapshot::embed_to_wire)
        .collect();
    wire.embeds.insert(0, proto::Embed::default());
    let Content::Post(read) = decode(&wire).unwrap() else {
        panic!("a text body is a post");
    };
    let urns: Vec<_> = read.embeds.iter().map(|embed| embed.urn.as_str()).collect();
    assert_eq!(
        urns,
        [
            "soundcloud:tracks:0",
            "soundcloud:tracks:1",
            "soundcloud:tracks:2"
        ]
    );
}

#[test]
fn empty_or_malformed_content_is_refused() {
    let mut embed_only = post(plain(" "));
    embed_only.embeds = vec![track("soundcloud:tracks:1")];
    assert!(encode(&Content::Post(embed_only)).is_ok());
    let mut blank_mention = post(plain("hi"));
    blank_mention.mentions = vec![String::new()];
    let mut stray_reply = post(plain("hi"));
    stray_reply.reply_to = Some(0);
    let mut playlist = track("soundcloud:playlists:1");
    playlist.kind = EmbedKind::Playlist;
    let refused = [
        Content::Post(post(plain("  "))),
        Content::Post(blank_mention),
        Content::Post(stray_reply),
        Content::Edit {
            target: 1,
            text: plain(" "),
        },
        Content::Delete { target: 0 },
        Content::Pin {
            target: -1,
            on: true,
        },
        Content::Reaction {
            target: 1,
            emoji: String::new(),
            on: true,
        },
        Content::Reaction {
            target: 1,
            emoji: "a\nb".into(),
            on: true,
        },
        Content::Reaction {
            target: 1,
            emoji: "🔥".repeat(17),
            on: true,
        },
        Content::Suggest(playlist),
    ];
    for content in refused {
        assert_eq!(
            encode(&content).err(),
            Some(ChatError::Invalid),
            "{content:?}"
        );
    }
}

#[test]
fn malformed_bodies_from_others_do_not_decode() {
    let bodies = [
        Body::Delete(proto::Delete { target: 0 }),
        Body::Pin(proto::Pin {
            target: u64::MAX,
            on: true,
        }),
        Body::Edit(proto::Edit {
            target: 1,
            text: None,
        }),
        Body::Reaction(proto::Reaction {
            target: 1,
            emoji: String::new(),
            on: true,
        }),
        Body::Suggest(proto::Suggest { track: None }),
    ];
    for body in bodies {
        let envelope = Envelope {
            body: Some(body),
            ..Envelope::default()
        };
        assert_eq!(decode(&envelope).err(), Some(ChatError::Protocol));
    }
}

fn at(class: MessageClass) -> MessageAt {
    MessageAt {
        conversation_id: Uuid::from_u128(1),
        epoch: 2,
        sender_device: Uuid::from_u128(3),
        client_id: Uuid::from_u128(4),
        class,
    }
}

#[test]
fn content_seals_only_under_its_own_class() {
    let ck = Ck::from_bytes([9; 32]);
    let reaction = Content::Reaction {
        target: 7,
        emoji: "🔥".into(),
        on: true,
    };
    let react = at(MessageClass::React);
    assert_eq!(
        seal(&ck, &at(MessageClass::Post), &reaction).err(),
        Some(ChatError::Invalid)
    );
    let body = seal(&ck, &react, &reaction).unwrap();
    assert_eq!(open(&ck, &react, Some(7), &body).unwrap(), reaction);
    assert_eq!(
        open(&ck, &react, None, &body).err(),
        Some(ChatError::Protocol)
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
    sc_chat_wire::msg::seal(ck.as_bytes(), &aad, [7; 24], envelope).unwrap()
}

#[test]
fn a_body_moved_to_another_class_by_its_sender_is_refused() {
    let ck = Ck::from_bytes([9; 32]);
    let post_at = at(MessageClass::Post);
    let edit = encode(&Content::Edit {
        target: 3,
        text: plain("sneaky"),
    })
    .unwrap();
    assert_eq!(
        seal_message(&ck, &post_at, &edit).err(),
        Some(ChatError::Invalid)
    );
    let body = forged(&ck, &post_at, &edit);
    assert_eq!(
        open(&ck, &post_at, None, &body).err(),
        Some(ChatError::Protocol)
    );
    let future = seal_message(&ck, &at(MessageClass::Edit), &Envelope::default()).unwrap();
    assert_eq!(
        open(&ck, &at(MessageClass::Edit), Some(3), &future).unwrap(),
        Content::Unknown
    );
}

#[test]
fn a_meta_message_opens_as_the_conversation_meta() {
    let ck = Ck::from_bytes([9; 32]);
    let meta = Content::Meta(ConversationMeta {
        title: "Ночная смена".into(),
        description: "треки после полуночи".into(),
        avatar_media_id: None,
    });
    let meta_at = at(MessageClass::Meta);
    let body = seal(&ck, &meta_at, &meta).unwrap();
    let stored = open_meta(&ck, meta_at.conversation_id, meta_at.epoch, &body).unwrap();
    assert_eq!(decode(&stored).unwrap(), meta);
    assert_eq!(open(&ck, &meta_at, None, &body).unwrap(), meta);
    assert_eq!(
        open_meta(&ck, Uuid::from_u128(5), meta_at.epoch, &body).err(),
        Some(ChatError::Crypto)
    );
}
