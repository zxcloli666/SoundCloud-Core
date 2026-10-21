mod store_support;

use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::store::{StoredContent, SyncState};
use sc_chat::{
    DeliveryState, HistoryQuery, Message, MessageBody, MessageClass, PeerMark, Span, SpanKind,
    SystemEvent, SystemKind,
};
use store_support::{ME, PEER, conv, edit, envelope, open, post, react, row, store, text};
use uuid::Uuid;

fn page(
    store: &sc_chat::store::Store,
    id: Uuid,
    before: Option<i64>,
    after: Option<i64>,
    limit: u32,
) -> (Vec<Message>, bool) {
    let page = store
        .messages(
            id,
            &HistoryQuery {
                before,
                after,
                limit,
            },
        )
        .unwrap();
    (page.messages, page.more)
}

fn seqs(messages: &[Message]) -> Vec<i64> {
    messages.iter().filter_map(|message| message.seq).collect()
}

fn body(message: &Message) -> &str {
    match &message.body {
        MessageBody::Text(text) => &text.text,
        _ => "",
    }
}

#[test]
fn pages_walk_back_and_forth() {
    let store = store();
    let id = conv();
    let rows: Vec<_> = (1..=7)
        .map(|seq| post(id, seq, PEER, &format!("m{seq}")))
        .collect();
    store.put_messages(&rows).unwrap();
    let (latest, more) = page(&store, id, None, None, 3);
    assert_eq!(seqs(&latest), vec![5, 6, 7]);
    assert!(more);
    let (older, more) = page(&store, id, Some(5), None, 3);
    assert_eq!(seqs(&older), vec![2, 3, 4]);
    assert!(more);
    let (oldest, more) = page(&store, id, Some(2), None, 3);
    assert_eq!(seqs(&oldest), vec![1]);
    assert!(!more);
    let (newer, more) = page(&store, id, None, Some(4), 2);
    assert_eq!(seqs(&newer), vec![5, 6]);
    assert!(more);
}

#[test]
fn modifiers_shape_the_message_but_never_show() {
    let store = store();
    let id = conv();
    let pin = row(
        id,
        6,
        PEER,
        MessageClass::Pin,
        Some(1),
        open(&envelope(Body::Pin(proto::Pin {
            target: 1,
            on: true,
        }))),
    );
    store
        .put_messages(&[
            post(id, 1, PEER, "исходный"),
            edit(id, 2, PEER, 1, "правленый"),
            react(id, 3, PEER, 1, "🔥", true),
            react(id, 4, ME, 1, "🔥", true),
            react(id, 5, PEER, 1, "👍", true),
            pin,
            react(id, 7, PEER, 1, "👍", false),
        ])
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(seqs(&messages), vec![1]);
    let message = &messages[0];
    assert_eq!(body(message), "правленый");
    assert_eq!(message.edited_at, Some(2_000));
    assert!(message.pinned);
    assert_eq!(message.reactions.len(), 1);
    assert_eq!(message.reactions[0].emoji, "🔥");
    assert_eq!(message.reactions[0].count, 2);
    assert!(message.reactions[0].mine);
    assert_eq!(store.pins(id).unwrap(), vec![1]);
}

#[test]
fn a_delete_envelope_blanks_the_target() {
    let store = store();
    let id = conv();
    let delete = row(
        id,
        2,
        PEER,
        MessageClass::Edit,
        Some(1),
        open(&envelope(Body::Delete(proto::Delete { target: 1 }))),
    );
    store
        .put_messages(&[
            store_support::post_with_track(id, 1, "смотри", "Nightcall"),
            delete,
        ])
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(messages[0].body, MessageBody::Deleted);
    assert!(messages[0].embeds.is_empty());
    let embeds = store
        .embeds(
            id,
            &sc_chat::EmbedsQuery {
                kinds: vec![],
                before: None,
                limit: 5,
            },
        )
        .unwrap();
    assert!(embeds.items.is_empty());
}

#[test]
fn mine_shows_ticks_from_peer_marks() {
    let store = store();
    let id = conv();
    store
        .put_messages(&[
            post(id, 1, ME, "a"),
            post(id, 2, ME, "b"),
            post(id, 3, ME, "c"),
            post(id, 4, PEER, "d"),
        ])
        .unwrap();
    store
        .put_marks(
            id,
            &[
                PeerMark {
                    user_id: PEER.into(),
                    read_seq: 1,
                    delivered_seq: 2,
                },
                PeerMark {
                    user_id: ME.into(),
                    read_seq: 4,
                    delivered_seq: 4,
                },
            ],
        )
        .unwrap();
    store
        .put_marks(
            id,
            &[PeerMark {
                user_id: PEER.into(),
                read_seq: 0,
                delivered_seq: 0,
            }],
        )
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    let states: Vec<_> = messages.iter().map(|message| message.state).collect();
    assert_eq!(
        states,
        vec![
            DeliveryState::Read,
            DeliveryState::Delivered,
            DeliveryState::Sent,
            DeliveryState::Sent
        ]
    );
    assert!(messages[0].mine);
    assert!(!messages[3].mine);
}

#[test]
fn mentions_of_me_are_flagged() {
    let store = store();
    let id = conv();
    let mut by_list = envelope(Body::Text(text("эй")));
    by_list.mentions = vec![ME.into()];
    let by_span = envelope(Body::Text(proto::Text {
        text: "@me".into(),
        spans: vec![proto::Span {
            kind: proto::SpanKind::Mention as i32,
            start: 0,
            len: 3,
            url: String::new(),
            user_id: ME.into(),
        }],
    }));
    store
        .put_messages(&[
            row(id, 1, PEER, MessageClass::Post, None, open(&by_list)),
            row(id, 2, PEER, MessageClass::Post, None, open(&by_span)),
            post(id, 3, PEER, "никого"),
        ])
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    let flags: Vec<_> = messages.iter().map(|message| message.mentions_me).collect();
    assert_eq!(flags, vec![true, true, false]);
    let MessageBody::Text(spanned) = &messages[1].body else {
        panic!("not text");
    };
    assert_eq!(
        spanned.spans,
        vec![Span {
            kind: SpanKind::Mention,
            start: 0,
            len: 3,
            url: None,
            user_id: Some(ME.into()),
        }]
    );
}

#[test]
fn sealed_rows_stay_locked_until_their_key_opens_them() {
    let store = store();
    let id = conv();
    let mut locked = row(
        id,
        1,
        PEER,
        MessageClass::Post,
        None,
        StoredContent::Sealed(vec![1; 40]),
    );
    locked.epoch = 3;
    store.put_messages(&[locked.clone()]).unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(messages[0].body, MessageBody::Locked);
    let waiting = store.sealed_messages(id, 3).unwrap();
    assert_eq!(waiting, vec![locked.clone()]);
    assert!(store.sealed_messages(id, 2).unwrap().is_empty());
    let mut opened = locked;
    opened.content = open(&envelope(Body::Text(text("открылось"))));
    store.put_messages(&[opened]).unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(body(&messages[0]), "открылось");
    assert!(store.sealed_messages(id, 3).unwrap().is_empty());
}

#[test]
fn system_rows_and_server_deletes_render_as_such() {
    let store = store();
    let id = conv();
    let event = SystemEvent {
        kind: SystemKind::Ttl,
        actor: Some(PEER.into()),
        target: None,
        name: None,
        ttl_s: Some(86_400),
        until: None,
        track: None,
        role: None,
        on: None,
    };
    store
        .put_messages(&[
            row(
                id,
                1,
                "",
                MessageClass::System,
                None,
                StoredContent::System(event.clone()),
            ),
            post(id, 2, PEER, "будет удалено"),
        ])
        .unwrap();
    store
        .put_messages(&[row(
            id,
            2,
            PEER,
            MessageClass::Post,
            None,
            StoredContent::Deleted,
        )])
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(messages[0].body, MessageBody::System(event));
    assert_eq!(messages[1].body, MessageBody::Deleted);
    let hits = sc_chat::search::Search::new(std::sync::Arc::new(store))
        .query(&sc_chat::SearchQuery {
            text: "удал".into(),
            conversation_id: None,
            limit: 5,
        })
        .unwrap();
    assert!(hits.is_empty());
}

#[test]
fn unknown_envelopes_fall_back() {
    let store = store();
    let id = conv();
    store
        .put_messages(&[
            row(
                id,
                1,
                PEER,
                MessageClass::Post,
                None,
                StoredContent::Open(vec![0xff, 0x01]),
            ),
            row(
                id,
                2,
                PEER,
                MessageClass::Post,
                None,
                open(&proto::Envelope::default()),
            ),
        ])
        .unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(messages[0].body, MessageBody::Unknown);
    assert_eq!(messages[1].body, MessageBody::Unknown);
}

#[test]
fn hidden_history_is_dropped_and_not_taken_back() {
    let store = store();
    let id = conv();
    store
        .put_conversations(&[store_support::summary(
            id,
            sc_chat::ConversationKind::Direct,
        )])
        .unwrap();
    store
        .put_messages(&[
            post(id, 1, PEER, "старое"),
            post(id, 2, PEER, "тоже"),
            post(id, 3, PEER, "новое"),
        ])
        .unwrap();
    store.hide_before(id, 2).unwrap();
    store.put_messages(&[post(id, 1, PEER, "старое")]).unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(seqs(&messages), vec![3]);
}

#[test]
fn sync_state_round_trips() {
    let store = store();
    assert_eq!(store.sync_state().unwrap(), SyncState::default());
    let state = SyncState {
        pos: 42,
        inc: Some(Uuid::new_v4()),
    };
    store.set_sync_state(state).unwrap();
    assert_eq!(store.sync_state().unwrap(), state);
    store
        .set_sync_state(SyncState { pos: 43, inc: None })
        .unwrap();
    assert_eq!(
        store.sync_state().unwrap(),
        SyncState { pos: 43, inc: None }
    );
}

fn deleted_marker(id: Uuid, seq: i64, target: i64) -> sc_chat::store::StoredMessage {
    let event = SystemEvent {
        kind: SystemKind::Deleted,
        actor: Some(PEER.into()),
        target: None,
        name: None,
        ttl_s: None,
        until: None,
        track: None,
        role: None,
        on: None,
    };
    row(
        id,
        seq,
        PEER,
        MessageClass::System,
        Some(target),
        StoredContent::System(event),
    )
}

#[test]
fn a_deleted_marker_scrubs_its_target_and_the_target_edits() {
    let store = store();
    let id = conv();
    store
        .put_messages(&[
            post(id, 1, PEER, "секретный план"),
            edit(id, 2, PEER, 1, "секретный план изменён"),
            post(id, 3, PEER, "остаётся"),
        ])
        .unwrap();
    store.put_messages(&[deleted_marker(id, 4, 1)]).unwrap();
    let (messages, _) = page(&store, id, None, None, 10);
    assert_eq!(messages[0].body, MessageBody::Deleted);
    assert_eq!(messages[0].edited_at, None);
    assert_eq!(body(&messages[1]), "остаётся");
    let search = sc_chat::search::Search::new(std::sync::Arc::new(store));
    let hits = search
        .query(&sc_chat::SearchQuery {
            text: "план".into(),
            conversation_id: None,
            limit: 10,
        })
        .unwrap();
    assert!(hits.is_empty());
}
