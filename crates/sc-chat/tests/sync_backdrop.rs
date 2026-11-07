mod room_support;
mod store_support;
mod sync_support;

use room_support::{
    R, X, Y, backdrop, carried_backdrop, conversation_gets, conversation_path, joined, keys_gets,
    live, meta_message, resolves, room_meta, room_view,
};
use sc_chat::MessageClass;
use sc_chat::api::wire::b64;
use sc_chat::crypto::{self, Ck, MessageAt};
use sc_chat::envelope::{self, Content};
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::Envelope;
use serde_json::json;
use sync_support::START_MS;
use sync_support::fixtures::post;
use sync_support::real::{CK, real};
use uuid::Uuid;

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
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
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
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(Y)));
    let synced = rig.events.syncs();
    let last = synced.last().unwrap();
    assert_eq!(
        (
            last.conversations[0].id,
            last.conversations[0].avatar_media_id
        ),
        (R, None)
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
    assert_eq!(carried_backdrop(&rig), None);
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
    assert_eq!(carried_backdrop(&rig), None);
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
async fn the_outbox_names_the_backdrop_a_room_meta_carries() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(None, 1)).await;
    let outbox = Outbox::new(rig.engine.store().clone());
    let send = format!("/chat/conversations/{R}/messages");
    for (n, shown) in [(1, Some(X)), (2, None)] {
        let envelope = envelope::encode(&Content::Meta(Box::new(room_meta(shown)))).unwrap();
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

#[tokio::test]
async fn a_meta_this_device_cannot_open_drops_the_old_backdrop() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));

    rig.server
        .json("GET", &format!("/chat/conversations/{R}/keys"), json!([]));
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(Y))), 2),
    );
    live(&rig, 2, 5, vec![meta_message(5, &room_meta(Some(Y)))]).await;

    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
    assert_eq!(resolves(&rig, X), None);
    assert_eq!(resolves(&rig, Y), None);
    assert_eq!(carried_backdrop(&rig), None);
    assert_eq!((conversation_gets(&rig), keys_gets(&rig)), (2, 2));
}
