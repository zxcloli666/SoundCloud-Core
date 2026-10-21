mod sync_support;

use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::store::SyncState;
use sc_chat::sync::Local;
use sc_chat::{ChatError, DeliveryState, MessageClass, MessagesOrigin, Tier};
use serde_json::json;
use sync_support::fixtures::{answer, entry, keys, summary, text};
use sync_support::{DEVICE, ME, START_MS, Step, ck_bytes, rig, rig_on};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);
const CLIENT: Uuid = Uuid::from_u128(0xc1);

fn queued(rig: &sync_support::Rig) {
    rig.local.put_conversations(&[summary(A, 6)]).unwrap();
    rig.local.queue(OutboxItem {
        client_id: CLIENT,
        conversation_id: A,
        class: MessageClass::Post,
        target_seq: None,
        envelope: text("hello").encode_to_vec(),
        created_at: START_MS,
        attempts: 0,
        next_at: START_MS,
    });
}

fn sends(rig: &sync_support::Rig) -> Vec<sync_support::Call> {
    rig.server
        .calls_to("POST", &format!("/chat/conversations/{A}/messages"))
}

#[tokio::test]
async fn a_lost_send_answer_is_replayed_on_the_next_tier_with_the_same_client_id() {
    let rig = rig();
    queued(&rig);
    let send = format!("/chat/conversations/{A}/messages");
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.on("POST", &send, Step::Fail(ChatError::Offline));
    rig.server.json(
        "POST",
        &send,
        json!({"seq": 7, "ts": START_MS + 7_000, "expires_at": null}),
    );

    assert_eq!(rig.engine.flush_outbox().await, Ok(1));

    let calls = sends(&rig);
    let tiers: Vec<Tier> = calls.iter().map(|call| call.tier).collect();
    assert_eq!(tiers, vec![Tier::Direct, Tier::Relay]);
    assert_eq!(calls[0].body["client_id"], calls[1].body["client_id"]);
    assert_eq!(rig.local.stored(A).len(), 1);
}

#[tokio::test]
async fn a_send_lost_on_every_tier_is_retried_later_with_the_same_client_id() {
    let rig = rig_on(&[Tier::Direct]);
    queued(&rig);
    let send = format!("/chat/conversations/{A}/messages");
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.on("POST", &send, Step::Fail(ChatError::Offline));
    rig.server.json(
        "POST",
        &send,
        json!({"seq": 7, "ts": START_MS + 7_000, "expires_at": null}),
    );

    assert_eq!(rig.engine.flush_outbox().await, Ok(0));
    let (waiting, failed) = rig.local.outbox()[0].clone();
    assert!(!failed);
    assert_eq!(waiting.attempts, 1);
    assert!(waiting.next_at > START_MS);
    assert_eq!(rig.engine.flush_outbox().await, Ok(0));

    rig.clock.advance(120_000);
    assert_eq!(rig.engine.flush_outbox().await, Ok(1));

    let calls = sends(&rig);
    assert_eq!(calls.len(), 2);
    for call in &calls {
        assert_eq!(call.body["client_id"], json!(CLIENT));
        assert_eq!(call.body["epoch"], json!(1));
        assert_eq!(call.body["class"], json!("post"));
    }
    let stored = rig.local.stored(A);
    assert_eq!(stored.len(), 1);
    assert_eq!((stored[0].seq, stored[0].client_id), (7, CLIENT));
    assert!(rig.local.outbox().is_empty());
    let local = &rig.events.messages()[0];
    assert_eq!(local.origin, MessagesOrigin::Local);
    assert_eq!(local.messages[0].seq, Some(7));
    assert_eq!(rig.local.summary(A).unwrap().last_seq, 7);
}

#[tokio::test]
async fn the_echo_of_a_sent_message_through_sync_stays_one_message() {
    let rig = rig_on(&[Tier::Direct]);
    queued(&rig);
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
    let send = format!("/chat/conversations/{A}/messages");
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.on("POST", &send, Step::Fail(ChatError::Offline));
    let mut echo = sync_support::fixtures::post_from(A, 7, 1, ME, "hello");
    echo["client_id"] = json!(CLIENT);
    echo["sender_device"] = json!(DEVICE);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 7, vec![echo])]),
    );

    rig.engine.flush_outbox().await.unwrap();
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();
    rig.clock.advance(120_000);
    assert_eq!(rig.engine.flush_outbox().await, Ok(0));

    assert_eq!(sends(&rig).len(), 1);
    assert_eq!(rig.local.stored(A).len(), 1);
    assert!(rig.local.outbox().is_empty());
    assert_eq!(rig.local.summary(A).unwrap().unread, 0);
}

#[tokio::test]
async fn a_stale_epoch_fetches_the_new_key_and_sends_again() {
    let rig = rig();
    queued(&rig);
    let send = format!("/chat/conversations/{A}/messages");
    let keys_path = format!("/chat/conversations/{A}/keys");
    rig.server.json("GET", &keys_path, keys(A, &[1]));
    rig.server.on(
        "POST",
        &send,
        Step::Json(409, json!({"error": "chat_epoch_stale"})),
    );
    rig.server.json("GET", &keys_path, keys(A, &[2]));
    rig.server.json(
        "POST",
        &send,
        json!({"seq": 7, "ts": START_MS, "expires_at": null}),
    );

    assert_eq!(rig.engine.flush_outbox().await, Ok(1));

    let epochs: Vec<_> = sends(&rig)
        .iter()
        .map(|call| call.body["epoch"].clone())
        .collect();
    assert_eq!(epochs, vec![json!(1), json!(2)]);
    let key_calls = rig.server.calls_to("GET", &keys_path);
    assert_eq!(key_calls[0].query, "from_epoch=0");
    assert_eq!(key_calls[1].query, "from_epoch=2");
    assert!(rig.local.has_key(A, 2));
}

#[tokio::test]
async fn a_refused_send_is_marked_failed_and_shown() {
    let rig = rig();
    queued(&rig);
    rig.local.add_key(A, 1, ck_bytes(A, 1));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.on(
        "POST",
        &format!("/chat/conversations/{A}/messages"),
        Step::Json(403, json!({"error": "chat_not_member"})),
    );

    assert_eq!(rig.engine.flush_outbox().await, Ok(0));

    assert!(rig.local.outbox()[0].1);
    let shown = &rig.events.messages()[0];
    assert_eq!(shown.origin, MessagesOrigin::Local);
    assert_eq!(shown.messages[0].state, DeliveryState::Failed);
}

#[tokio::test]
async fn a_revoked_device_stops_the_outbox() {
    let rig = rig();
    queued(&rig);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.on(
        "POST",
        &format!("/chat/conversations/{A}/messages"),
        Step::Json(403, json!({"error": "chat_device_revoked"})),
    );
    assert_eq!(
        rig.engine.flush_outbox().await,
        Err(ChatError::DeviceRevoked)
    );
    assert!(!rig.local.outbox()[0].1);
}

#[tokio::test]
async fn a_send_that_lands_after_unseen_messages_keeps_the_cursor_and_the_server_timer() {
    let rig = rig_on(&[Tier::Direct]);
    queued(&rig);
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
    let expires_at = START_MS + 3_600_000;
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server.json(
        "POST",
        &format!("/chat/conversations/{A}/messages"),
        json!({"seq": 9, "ts": START_MS, "expires_at": expires_at}),
    );
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));

    assert_eq!(rig.engine.flush_outbox().await, Ok(1));
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();

    let stored = rig.local.stored(A);
    assert_eq!((stored[0].seq, stored[0].expires_at), (9, Some(expires_at)));
    assert_eq!(rig.local.summary(A).unwrap().last_seq, 6);
    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["known"], json!([[A, 6]]));
}
