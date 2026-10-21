mod sync_support;

use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::store::{StoredContent, StoredMessage, SyncState};
use sc_chat::sync::{Local, SyncOutcome};
use sc_chat::{
    ConversationKind, GoneMessage, MessageBody, MessageClass, MessagesOrigin, PreviewKind,
    SystemKind, Tier,
};
use serde_json::json;
use sync_support::fixtures::{answer, card, entry, keys, post, summary, system, text, view};
use sync_support::{ME, PEER, PEER_DEVICE, START_MS, rig};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);
const B: Uuid = Uuid::from_u128(0xb);
const C: Uuid = Uuid::from_u128(0xc);

fn synced(rig: &sync_support::Rig) {
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
}

fn open(conversation_id: Uuid, seq: i64, body: &str) -> StoredMessage {
    StoredMessage {
        conversation_id,
        seq,
        client_id: Uuid::from_u128(seq as u128 + 0x1000),
        sender: PEER.into(),
        sender_device: Some(PEER_DEVICE),
        class: MessageClass::Post,
        epoch: 1,
        target_seq: None,
        ts: START_MS + seq * 1000,
        expires_at: None,
        local_only: false,
        content: StoredContent::Open(text(body).encode_to_vec()),
    }
}

fn plan(rig: &sync_support::Rig) -> sc_chat::sync::PollPlan {
    rig.engine.plan(Tier::Direct)
}

#[tokio::test]
async fn first_sync_resets_and_reconciles_the_list() {
    let rig = rig();
    rig.local.put_conversations(&[summary(B, 4)]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    assert!(rig.engine.reset_pending());

    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 3)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two"), post(A, 3, 1, "three")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.engine.reconcile().await.unwrap();

    assert!(!rig.engine.reset_pending());
    assert_eq!(rig.local.summary(B), None);
    assert_eq!(rig.events.gone()[0].conversations, vec![B]);
    let a = rig.local.summary(A).unwrap();
    assert_eq!(a.last_seq, 3);
    assert_eq!(a.unread, 3);
    assert_eq!(a.peer.unwrap().name, "peer");
    let preview = a.preview.unwrap();
    assert_eq!(
        (preview.seq, preview.kind, preview.text.as_str()),
        (3, PreviewKind::Text, "three")
    );
    assert_eq!(
        rig.local.sync_state().unwrap(),
        SyncState {
            pos: 5,
            inc: Some(INC)
        }
    );
    let reset = rig
        .events
        .syncs()
        .into_iter()
        .find(|sync| sync.reset)
        .unwrap();
    assert_eq!(reset.conversations.len(), 1);
    let history = &rig
        .server
        .calls_to("GET", &format!("/chat/conversations/{A}/messages"))[0];
    assert_eq!(history.query, "limit=50");
    assert!(
        rig.local
            .stored(A)
            .iter()
            .all(|message| matches!(message.content, StoredContent::Open(_)))
    );
}

#[tokio::test]
async fn gone_purges_conversations_and_expired_messages() {
    let rig = rig();
    synced(&rig);
    rig.local
        .put_conversations(&[summary(A, 3), summary(B, 1)])
        .unwrap();
    rig.local
        .put_messages(&[
            open(A, 1, "one"),
            open(A, 2, "two"),
            open(A, 3, "three"),
            open(B, 1, "b"),
        ])
        .unwrap();
    let mut gone = answer(2, INC, vec![]);
    gone["gone"] = json!([B]);
    gone["gone_msgs"] = json!([[A, 3]]);
    rig.server.json("POST", "/chat/sync", gone);

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    assert_eq!(rig.local.summary(B), None);
    assert!(rig.local.stored(B).is_empty());
    let left: Vec<i64> = rig
        .local
        .stored(A)
        .iter()
        .map(|message| message.seq)
        .collect();
    assert_eq!(left, vec![1, 2]);
    let event = &rig.events.gone()[0];
    assert_eq!(event.conversations, vec![B]);
    assert_eq!(
        event.messages,
        vec![GoneMessage {
            conversation_id: A,
            seq: 3
        }]
    );
    assert_eq!(rig.local.summary(A).unwrap().preview.unwrap().seq, 2);
    assert_eq!(rig.local.sync_state().unwrap().pos, 2);
}

#[tokio::test]
async fn expiring_messages_keep_their_deadline() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    let mut expiring = post(A, 2, 1, "soon gone");
    expiring["expires_at"] = json!(START_MS + 3_600_000);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![expiring])]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    let stored = rig.local.stored(A);
    assert_eq!(stored[0].expires_at, Some(START_MS + 3_600_000));
    let event = &rig.events.messages()[0];
    assert_eq!(event.origin, MessagesOrigin::Sync);
    assert_eq!(event.messages[0].expires_at, Some(START_MS + 3_600_000));
}

#[tokio::test]
async fn incarnation_change_keeps_local_rows_above_the_server() {
    let rig = rig();
    rig.local
        .set_sync_state(SyncState {
            pos: 40,
            inc: Some(Uuid::from_u128(0x01d)),
        })
        .unwrap();
    rig.local.put_conversations(&[summary(A, 10)]).unwrap();
    rig.local
        .put_messages(&[open(A, 8, "eight"), open(A, 9, "nine"), open(A, 10, "ten")])
        .unwrap();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(3, INC, vec![entry(A, 8, vec![])]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    assert_eq!(rig.local.stored(A).len(), 3);

    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 8)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.engine.reconcile().await.unwrap();

    assert_eq!(rig.local.kept_above(), vec![(A, 8)]);
    let local_only: Vec<bool> = rig
        .local
        .stored(A)
        .iter()
        .map(|message| message.local_only)
        .collect();
    assert_eq!(local_only.iter().filter(|kept| **kept).count(), 2);
    assert_eq!(
        rig.local.sync_state().unwrap(),
        SyncState {
            pos: 3,
            inc: Some(INC)
        }
    );
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}/messages"))
            .is_empty()
    );
}

#[tokio::test]
async fn an_unknown_conversation_loads_its_view_and_newest_page() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![])]),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    let a = rig.local.summary(A).unwrap();
    assert_eq!((a.last_seq, a.unread), (2, 2));
    let event = &rig.events.messages()[0];
    assert_eq!(event.messages.len(), 2);
    assert!(matches!(&event.messages[1].body, MessageBody::Text(rich) if rich.text == "two"));
    let sync = rig.events.syncs().pop().unwrap();
    assert_eq!(sync.conversations[0].id, A);
}

#[tokio::test]
async fn system_rows_of_both_shapes_are_parsed() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            2,
            INC,
            vec![entry(
                A,
                2,
                vec![
                    system(1, json!({"kind": "ttl", "actor": PEER, "ttl_s": 86400})),
                    system(2, json!({"type": "kick", "user_id": ME, "name": "me", "by": PEER, "ban": true})),
                ],
            )],
        ),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    let stored = rig.local.stored(A);
    let StoredContent::System(ttl) = &stored[0].content else {
        panic!("not a system row");
    };
    assert_eq!(
        (ttl.kind, ttl.ttl_s, ttl.actor.as_deref()),
        (SystemKind::Ttl, Some(86400), Some(PEER))
    );
    let StoredContent::System(kick) = &stored[1].content else {
        panic!("not a system row");
    };
    assert_eq!(kick.kind, SystemKind::Banned);
    assert_eq!(
        (kick.actor.as_deref(), kick.target.as_deref()),
        (Some(PEER), Some(ME))
    );
    assert_eq!(rig.local.summary(A).unwrap().unread, 0);
}

#[tokio::test]
async fn role_delete_moderator_and_suggest_rows_keep_their_kind() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    let track = json!({"urn": "soundcloud:tracks:1", "title": "Nightcall", "artist": "Kavinsky"});
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            4,
            INC,
            vec![entry(
                A,
                4,
                vec![
                    system(1, json!({"kind": "role", "actor": PEER, "target": ME, "role": "admin"})),
                    system(2, json!({"kind": "deleted", "actor": PEER})),
                    system(3, json!({"type": "moderator", "user_id": ME, "name": "me", "by": PEER, "on": false})),
                    system(4, json!({"type": "suggest", "user_id": PEER, "name": "peer", "track": track})),
                ],
            )],
        ),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 4));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    let events: Vec<sc_chat::SystemEvent> = rig
        .local
        .stored(A)
        .into_iter()
        .filter_map(|row| match row.content {
            StoredContent::System(event) => Some(event),
            _ => None,
        })
        .collect();
    let kinds: Vec<SystemKind> = events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            SystemKind::Role,
            SystemKind::Deleted,
            SystemKind::Moderator,
            SystemKind::Suggest
        ]
    );
    assert_eq!(events[0].role, Some(sc_chat::MemberRole::Admin));
    assert_eq!(events[0].target.as_deref(), Some(ME));
    assert_eq!(
        (events[2].on, events[2].target.as_deref()),
        (Some(false), Some(ME))
    );
    assert_eq!(
        events[3].track.as_ref().map(|track| track.title.as_str()),
        Some("Nightcall")
    );
}

struct SavedProbe {
    local: std::sync::Arc<sync_support::MemoryLocal>,
    saved: std::sync::Mutex<Vec<bool>>,
}

impl sc_chat::Events for SavedProbe {
    fn emit(&self, event: sc_chat::CoreEvent) {
        if let sc_chat::CoreEvent::Messages(shown) = event {
            let saved = self.local.summary(shown.conversation_id).is_some();
            self.saved.lock().unwrap().push(saved);
        }
    }
}

#[tokio::test]
async fn a_reset_shows_messages_of_a_new_chat_only_after_the_chat_is_saved() {
    let rig = rig();
    let probe = std::sync::Arc::new(SavedProbe {
        local: rig.local.clone(),
        saved: std::sync::Mutex::default(),
    });
    let engine = sc_chat::sync::SyncEngine::with_parts(
        rig.engine.api().clone(),
        rig.local.clone(),
        probe.clone(),
        std::sync::Arc::new(sync_support::TestCipher),
    );
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 1)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );

    engine.reconcile().await.unwrap();

    assert_eq!(*probe.saved.lock().unwrap(), [true]);
}

#[tokio::test]
async fn read_marks_ride_the_next_sync_once() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 5)]).unwrap();
    rig.engine.queue_read(A, 5);
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Idle));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Idle));

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["read"], json!([[A, 5]]));
    assert_eq!(syncs[1].body["read"], json!([]));
    assert_eq!(syncs[0].body["known"], json!([[A, 5]]));
    assert_eq!(rig.local.summary(A).unwrap().last_read_seq, 5);
}

#[tokio::test]
async fn received_messages_are_marked_delivered_and_typing_reaches_the_focus() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    rig.engine.set_visible(true);
    rig.engine.set_focus(Some(A));
    let mut live = entry(A, 2, vec![post(A, 2, 1, "hey")]);
    live["typing"] = json!([PEER]);
    live["marks"] = json!([{"user_id": PEER, "read_seq": 1, "delivered_seq": 2}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![live]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["focus"], json!(A));
    assert_eq!(syncs[1].body["delivered"], json!([[A, 2]]));
    assert_eq!(rig.events.typing()[0].users, vec![PEER.to_owned()]);
    assert_eq!(rig.local.marks_of(A)[0].delivered_seq, 2);
    let marks = &rig.events.syncs()[0].marks[0];
    assert_eq!(marks.conversation_id, A);
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}"))
            .is_empty()
    );
}

#[tokio::test]
async fn a_cut_answer_polls_again_without_holding() {
    let rig = rig();
    synced(&rig);
    rig.engine.set_visible(true);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    let mut cut = entry(A, 9, vec![post(A, 2, 1, "two")]);
    cut["more"] = json!(true);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![cut]));

    rig.engine.step(plan(&rig)).await.unwrap();

    let next = rig.engine.plan(Tier::Direct);
    assert_eq!((next.hold_ms, next.delay_ms), (0, 0));
    assert_eq!(rig.local.summary(A).unwrap().last_seq, 2);
}

#[tokio::test]
async fn the_badge_counts_loud_unread_chats_and_every_mention() {
    let rig = rig();
    synced(&rig);
    let mut loud = summary(A, 1);
    loud.unread = 1;
    let mut muted = summary(B, 3);
    muted.unread = 3;
    muted.mentions = 1;
    muted.muted_until = Some(START_MS + 3_600_000);
    let mut room = summary(C, 2);
    room.kind = ConversationKind::Room;
    room.unread = 2;
    rig.local.put_conversations(&[loud, muted, room]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));

    rig.engine.load_tally().unwrap();
    let status = rig.engine.status();
    assert_eq!((status.unread, status.mentions), (1, 1));

    rig.engine.queue_read(A, 1);
    assert_eq!(rig.engine.status().unread, 0);

    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![post(A, 2, 1, "again")])]),
    );
    rig.engine.step(plan(&rig)).await.unwrap();
    assert_eq!(rig.engine.status().unread, 1);

    let mut gone = answer(3, INC, vec![]);
    gone["gone"] = json!([A, B]);
    rig.server.json("POST", "/chat/sync", gone);
    rig.engine.step(plan(&rig)).await.unwrap();
    let status = rig.engine.status();
    assert_eq!((status.unread, status.mentions), (0, 0));
}

#[tokio::test]
async fn a_hidden_window_sends_no_focus() {
    let rig = rig();
    synced(&rig);
    rig.engine.set_focus(Some(A));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.set_visible(true);
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["focus"], json!(null));
    assert_eq!(syncs[1].body["focus"], json!(A));
}

const ROOM: Uuid = Uuid::from_u128(0xe);

#[tokio::test]
async fn a_room_chat_is_adopted_from_its_seat_and_never_asks_for_a_conversation_view() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/messages"),
        json!({"messages": [post(ROOM, 6, 1, "seated")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/keys"),
        keys(ROOM, &[1]),
    );
    let adopted = rig.engine.adopt_room(ROOM, "ROOM42", 5).await.unwrap();
    assert_eq!(adopted.kind, ConversationKind::Room);
    assert_eq!(adopted.room_code.as_deref(), Some("ROOM42"));
    assert_eq!(adopted.last_seq, 6);
    let history = &rig
        .server
        .calls_to("GET", &format!("/chat/conversations/{ROOM}/messages"))[0];
    assert!(history.query.contains("after=5"), "{}", history.query);

    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            3,
            INC,
            vec![entry(
                ROOM,
                7,
                vec![system(
                    7,
                    json!({"type": "join", "user_id": PEER, "name": "peer"}),
                )],
            )],
        ),
    );
    rig.engine.step(plan(&rig)).await.unwrap();
    assert_eq!(rig.local.summary(ROOM).unwrap().last_seq, 7);
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{ROOM}"))
            .is_empty()
    );
}

#[tokio::test]
async fn a_reset_keeps_room_chats_the_list_never_shows_and_catches_them_up() {
    let rig = rig();
    let mut room = summary(ROOM, 2);
    room.kind = ConversationKind::Room;
    room.room_code = Some("ROOM42".into());
    rig.local.put_conversations(&[room]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    for _ in 0..2 {
        rig.server.json(
            "GET",
            &format!("/chat/conversations/{ROOM}/messages"),
            json!({"messages": [post(ROOM, 3, 1, "while away")], "more": false}),
        );
    }
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/keys"),
        keys(ROOM, &[1]),
    );

    rig.engine.reconcile().await.unwrap();

    assert!(rig.events.gone().is_empty());
    assert_eq!(rig.local.summary(ROOM).unwrap().last_seq, 3);
    let shown = rig.events.messages().pop().unwrap();
    assert_eq!(shown.conversation_id, ROOM);
}

#[tokio::test]
async fn a_reset_purges_a_room_chat_whose_room_ended_while_away() {
    let rig = rig();
    let mut room = summary(ROOM, 2);
    room.kind = ConversationKind::Room;
    room.room_code = Some("ROOM42".into());
    rig.local.put_conversations(&[room]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    rig.server.on(
        "GET",
        &format!("/chat/conversations/{ROOM}/messages"),
        sync_support::Step::Json(403, json!({"error": "chat_not_member"})),
    );

    rig.engine.reconcile().await.unwrap();

    assert!(rig.local.summary(ROOM).is_none());
    assert!(
        rig.events
            .gone()
            .iter()
            .any(|gone| gone.conversations.contains(&ROOM))
    );
}

#[tokio::test]
async fn a_new_chat_caught_up_from_history_is_marked_delivered() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![])]),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[1].body["delivered"], json!([[A, 2]]));
}

#[tokio::test]
async fn a_bump_that_carries_only_my_settled_mark_refreshes_the_chat() {
    let rig = rig();
    synced(&rig);
    let mut invited = summary(A, 3);
    invited.accepted = false;
    invited.last_read_seq = 3;
    rig.local.put_conversations(&[invited]).unwrap();
    let mut bump = entry(A, 3, vec![]);
    bump["marks"] = json!([{"user_id": ME, "read_seq": 3, "delivered_seq": 3}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![bump]));
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 3));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    assert!(rig.local.summary(A).unwrap().accepted);
}

#[tokio::test]
async fn my_read_mark_from_another_device_moves_without_a_refresh() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 5)]).unwrap();
    let mut read = entry(A, 5, vec![]);
    read["marks"] = json!([{"user_id": ME, "read_seq": 5, "delivered_seq": 5}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![read]));

    rig.engine.step(plan(&rig)).await.unwrap();

    assert_eq!(rig.local.summary(A).unwrap().last_read_seq, 5);
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}"))
            .is_empty()
    );
}
