mod store_support;
mod sync_support;

use sc_chat::api::wire::b64;
use sc_chat::crypto::{self, Ck, MessageAt};
use sc_chat::envelope::{self, Content};
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::{Attachment, Envelope};
use sc_chat::{ChatError, ConversationMeta, MessageClass};
use serde_json::{Value, json};
use store_support::avatar;
use sync_support::fixtures::{answer, entry, post, view};
use sync_support::real::{CK, INC, Real, direct_plan, real};
use sync_support::{PEER, PEER_DEVICE, START_MS, Step};
use uuid::Uuid;

const R: Uuid = Uuid::from_u128(0x7e);
const X: u128 = 0x61;
const Y: u128 = 0x62;

fn backdrop(n: u128) -> Attachment {
    let mut attachment = avatar(n);
    attachment.size = 409_600;
    attachment.w = 1920;
    attachment.h = 1080;
    attachment
}

fn room_meta(n: Option<u128>) -> ConversationMeta {
    ConversationMeta {
        avatar: n.map(backdrop),
        ..ConversationMeta::default()
    }
}

fn room_view(meta: Option<&ConversationMeta>, epoch: u32) -> Value {
    let mut view = view(R, 0);
    view["kind"] = json!("room");
    view["peer"] = Value::Null;
    if let Some(meta) = meta {
        let envelope = envelope::encode(&Content::Meta(meta.clone())).unwrap();
        let meta_ct = crypto::seal_meta(&Ck::from_bytes(CK), R, epoch, &envelope).unwrap();
        view["meta_ct"] = json!(b64::encode(&meta_ct));
        view["meta_epoch"] = json!(epoch);
    }
    view
}

fn meta_message(seq: i64, meta: &ConversationMeta) -> Value {
    let client_id = Uuid::from_u128(0x900 + seq as u128);
    let at = MessageAt {
        conversation_id: R,
        epoch: 1,
        sender_device: PEER_DEVICE,
        client_id,
        class: MessageClass::Meta,
    };
    let envelope = envelope::encode(&Content::Meta(meta.clone())).unwrap();
    let sealed = crypto::seal_message(&Ck::from_bytes(CK), &at, &envelope).unwrap();
    json!({
        "seq": seq,
        "client_id": client_id,
        "sender": PEER,
        "sender_device": PEER_DEVICE,
        "class": "meta",
        "epoch": 1,
        "target_seq": null,
        "body": b64::encode(&sealed),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

fn conversation_path() -> String {
    format!("/chat/conversations/{R}")
}

fn conversation_gets(rig: &Real) -> usize {
    rig.server.calls_to("GET", &conversation_path()).len()
}

fn keys_gets(rig: &Real) -> usize {
    rig.server
        .calls_to("GET", &format!("/chat/conversations/{R}/keys"))
        .len()
}

async fn joined(rig: &Real, view: Value) {
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{R}/messages"),
        json!({"messages": [], "more": false}),
    );
    rig.server.json("GET", &conversation_path(), view);
    rig.engine.adopt_room(R, "ROOM42", 4).await.unwrap();
}

async fn live(rig: &Real, pos: u64, last_seq: i64, messages: Vec<Value>) {
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(pos, INC, vec![entry(R, last_seq, messages)]),
    );
    rig.engine.step(direct_plan()).await.unwrap();
}

fn resolves(rig: &Real, n: u128) -> Option<Attachment> {
    rig.engine
        .store()
        .attachment(R, Uuid::from_u128(n))
        .unwrap()
}

fn shown_backdrop(rig: &Real) -> Option<Uuid> {
    rig.engine
        .store()
        .conversation(R)
        .unwrap()
        .unwrap()
        .avatar_media_id
}

#[tokio::test]
async fn a_member_who_joins_later_resolves_the_current_backdrop() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;

    assert_eq!(
        rig.engine.store().room_backdrop(R).unwrap(),
        Some(backdrop(X))
    );
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(shown_backdrop(&rig), Some(Uuid::from_u128(X)));
    assert_eq!(conversation_gets(&rig), 1);
}

#[tokio::test]
async fn the_host_replaces_then_clears_and_every_member_follows_with_one_fetch_per_batch() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;

    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(Y))), 1),
    );
    let replaced = vec![
        meta_message(5, &room_meta(Some(X))),
        meta_message(6, &room_meta(Some(Y))),
        post(R, 7, 1, "nice"),
    ];
    live(&rig, 2, 7, replaced).await;
    assert_eq!(conversation_gets(&rig), 2);
    assert_eq!(resolves(&rig, X), None);
    assert_eq!(resolves(&rig, Y), Some(backdrop(Y)));
    assert_eq!(shown_backdrop(&rig), Some(Uuid::from_u128(Y)));
    let synced = rig.events.syncs();
    let last = synced.last().unwrap();
    assert_eq!(
        last.conversations[0].avatar_media_id,
        Some(Uuid::from_u128(Y))
    );

    live(&rig, 3, 8, vec![post(R, 8, 1, "no meta here")]).await;
    assert_eq!(conversation_gets(&rig), 2);

    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(None)), 1),
    );
    live(&rig, 4, 9, vec![meta_message(9, &room_meta(None))]).await;
    assert_eq!(conversation_gets(&rig), 3);
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
    assert_eq!(resolves(&rig, Y), None);
    assert_eq!(shown_backdrop(&rig), None);
    assert_eq!(keys_gets(&rig), 1);
}

#[tokio::test]
async fn a_missing_key_leaves_the_backdrop_unresolved_without_a_retry_loop() {
    let rig = real(&[]);
    rig.server
        .json("GET", &format!("/chat/conversations/{R}/keys"), json!([]));
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
    assert_eq!(resolves(&rig, X), None);
    assert_eq!(shown_backdrop(&rig), None);
    assert_eq!((conversation_gets(&rig), keys_gets(&rig)), (1, 1));

    for (pos, seq) in [(2, 5), (3, 6), (4, 7)] {
        live(&rig, pos, seq, vec![post(R, seq, 1, "chat")]).await;
    }
    assert_eq!((conversation_gets(&rig), keys_gets(&rig)), (1, 1));

    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(Y))), 1),
    );
    live(&rig, 5, 8, vec![meta_message(8, &room_meta(Some(Y)))]).await;
    assert_eq!((conversation_gets(&rig), keys_gets(&rig)), (2, 1));
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
}

#[tokio::test]
async fn a_dropped_meta_fetch_is_retried_with_the_next_batch_only() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{R}/messages"),
        json!({"messages": [], "more": false}),
    );
    rig.server
        .on("GET", &conversation_path(), Step::Fail(ChatError::Offline));
    rig.engine.adopt_room(R, "ROOM42", 4).await.unwrap();
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);

    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );
    live(&rig, 2, 5, vec![post(R, 5, 1, "back online")]).await;
    assert_eq!(conversation_gets(&rig), 2);
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));

    live(&rig, 3, 6, vec![post(R, 6, 1, "again")]).await;
    assert_eq!(conversation_gets(&rig), 2);
}

#[tokio::test]
async fn a_room_without_any_meta_drops_a_stale_backdrop() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));

    joined(&rig, room_view(None, 1)).await;
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
    assert_eq!(resolves(&rig, X), None);
    assert_eq!(shown_backdrop(&rig), None);
}

#[tokio::test]
async fn the_outbox_names_the_backdrop_a_room_meta_carries() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(None, 1)).await;
    let outbox = Outbox::new(rig.engine.store().clone());
    let send = format!("/chat/conversations/{R}/messages");
    for (n, shown) in [(1, Some(X)), (2, None)] {
        let envelope = envelope::encode(&Content::Meta(room_meta(shown))).unwrap();
        outbox
            .push(OutboxItem {
                client_id: Uuid::from_u128(0xc0 + n),
                conversation_id: R,
                class: MessageClass::Meta,
                target_seq: None,
                envelope: envelope.encode_to_vec(),
                created_at: START_MS + n as i64,
                attempts: 0,
                next_at: START_MS,
            })
            .unwrap();
        rig.server.json(
            "POST",
            &send,
            json!({"seq": 4 + n, "ts": START_MS + n as i64, "expires_at": null}),
        );
    }

    assert_eq!(rig.engine.flush_outbox().await, Ok(2));

    let calls = rig.server.calls_to("POST", &send);
    assert_eq!(calls[0].body["class"], "meta");
    assert_eq!(calls[0].body["media"], json!([Uuid::from_u128(X)]));
    assert!(calls[1].body.get("media").is_none());
    let sealed = b64::decode(calls[0].body["body"].as_str().unwrap()).unwrap();
    let opened: Envelope = crypto::open_message(
        &Ck::from_bytes(CK),
        &MessageAt {
            conversation_id: R,
            epoch: 1,
            sender_device: sync_support::DEVICE,
            client_id: Uuid::from_u128(0xc1),
            class: MessageClass::Meta,
        },
        &sealed,
    )
    .unwrap();
    assert_eq!(opened.attachments, vec![backdrop(X)]);
}
