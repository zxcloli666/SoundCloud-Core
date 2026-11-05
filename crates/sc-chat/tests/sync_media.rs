mod store_support;
mod sync_support;

use sc_chat::api::wire::{ConversationView, b64};
use sc_chat::crypto::{self, Ck, MessageAt};
use sc_chat::envelope::{self, Content};
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::Envelope;
use sc_chat::store::{NewUpload, UploadState};
use sc_chat::sync::PollPlan;
use sc_chat::{ConversationMeta, HistoryQuery, MediaKind, MessageClass};
use serde_json::{Value, json};
use store_support::{avatar, image};
use sync_support::fixtures::{answer, entry, summary, text, view};
use sync_support::real::{CK, INC, Real, direct_plan};
use sync_support::{PEER, PEER_DEVICE, START_MS};
use uuid::Uuid;

const A: Uuid = Uuid::from_u128(0xa);

fn real() -> Real {
    let rig = sync_support::real::real(&[summary(A, 0)]);
    rig.serve_keys(A, &[1]);
    rig
}

fn plan() -> PollPlan {
    direct_plan()
}

fn peer_album(seq: i64, ids: &[u128]) -> Value {
    let client_id = Uuid::from_u128(0x500 + seq as u128);
    let at = MessageAt {
        conversation_id: A,
        epoch: 1,
        sender_device: PEER_DEVICE,
        client_id,
        class: MessageClass::Post,
    };
    let mut envelope = text("");
    envelope.attachments = ids.iter().copied().map(image).collect();
    let sealed = crypto::seal_message(&Ck::from_bytes(CK), &at, &envelope).unwrap();
    json!({
        "seq": seq,
        "client_id": client_id,
        "sender": PEER,
        "sender_device": PEER_DEVICE,
        "class": "post",
        "epoch": 1,
        "target_seq": null,
        "body": b64::encode(&sealed),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

fn group_view(meta: &ConversationMeta) -> ConversationView {
    let envelope = envelope::encode(&Content::Meta(meta.clone())).unwrap();
    let meta_ct = crypto::seal_meta(&Ck::from_bytes(CK), A, 1, &envelope).unwrap();
    let mut view = view(A, 0);
    view["kind"] = json!("group");
    view["peer"] = Value::Null;
    view["meta_ct"] = json!(b64::encode(&meta_ct));
    view["meta_epoch"] = json!(1);
    serde_json::from_value(view).unwrap()
}

fn meta(avatar: Option<u128>) -> ConversationMeta {
    ConversationMeta {
        title: "Ночная смена".into(),
        description: String::new(),
        avatar: avatar.map(store_support::avatar),
    }
}

#[tokio::test]
async fn a_synced_album_lists_its_images_and_resolves_their_keys_in_core() {
    let rig = real();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![peer_album(1, &[1, 2])])]),
    );

    rig.engine.step(plan()).await.unwrap();

    let query = HistoryQuery {
        before: None,
        after: None,
        limit: 10,
    };
    let store = rig.engine.store();
    let message = store.messages(A, &query).unwrap().messages.remove(0);
    let ids: Vec<Uuid> = message
        .attachments
        .iter()
        .map(|attachment| attachment.media_id)
        .collect();
    assert_eq!(ids, vec![Uuid::from_u128(1), Uuid::from_u128(2)]);
    assert_eq!(
        store.attachment(A, Uuid::from_u128(1)).unwrap(),
        Some(image(1))
    );
    let shown = &rig.events.messages()[0].messages[0];
    assert_eq!(shown.attachments, message.attachments);
}

#[tokio::test]
async fn a_group_avatar_comes_from_the_meta_and_goes_with_it() {
    let rig = real();
    let store = rig.engine.store();

    let summary = rig.engine.adopt(&group_view(&meta(Some(9)))).await.unwrap();
    assert_eq!(summary.title.as_deref(), Some("Ночная смена"));
    assert_eq!(summary.avatar_media_id, Some(Uuid::from_u128(9)));
    assert_eq!(
        store.attachment(A, Uuid::from_u128(9)).unwrap(),
        Some(avatar(9))
    );

    let mut plain = group_view(&meta(Some(9)));
    plain.meta_ct = None;
    plain.meta_epoch = None;
    let kept = rig.engine.adopt(&plain).await.unwrap();
    assert_eq!(kept.avatar_media_id, Some(Uuid::from_u128(9)));

    let cleared = rig.engine.adopt(&group_view(&meta(None))).await.unwrap();
    assert_eq!(cleared.avatar_media_id, None);
    assert_eq!(store.attachment(A, Uuid::from_u128(9)).unwrap(), None);
    assert_eq!(store.group_meta(A).unwrap(), Some(meta(None)));
}

#[tokio::test]
async fn a_meta_message_names_the_avatar_it_carries() {
    let rig = real();
    let outbox = Outbox::new(rig.engine.store().clone());
    let send = format!("/chat/conversations/{A}/messages");
    for (n, avatar) in [(1, Some(9)), (2, None)] {
        let envelope = envelope::encode(&Content::Meta(meta(avatar))).unwrap();
        outbox
            .push(OutboxItem {
                client_id: Uuid::from_u128(0xc0 + n),
                conversation_id: A,
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
            json!({"seq": n, "ts": START_MS + n as i64, "expires_at": null}),
        );
    }

    assert_eq!(rig.engine.flush_outbox().await, Ok(2));

    let calls = rig.server.calls_to("POST", &send);
    assert_eq!(calls[0].body["class"], "meta");
    assert_eq!(calls[0].body["media"], json!([Uuid::from_u128(9)]));
    assert!(calls[1].body.get("media").is_none());
    let sealed = b64::decode(calls[0].body["body"].as_str().unwrap()).unwrap();
    let opened: Envelope = crypto::open_meta(&Ck::from_bytes(CK), A, 1, &sealed).unwrap();
    assert_eq!(opened.attachments, vec![avatar(9)]);
}

#[tokio::test]
async fn the_engine_announces_upload_changes() {
    let rig = real();
    let store = rig.engine.store();
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: A,
        kind: MediaKind::Image,
        image_ref: "media/a.img".into(),
        thumb_ref: Some("media/a.thumb".into()),
        attachment: image(0),
    };
    let id = store.enqueue_upload(&upload, START_MS).unwrap();
    store.upload_failed(id, false, START_MS + 1).unwrap();
    store.remove_album_uploads(upload.album).unwrap();

    let seen: Vec<_> = rig
        .events
        .uploads()
        .into_iter()
        .map(|event| {
            let states: Vec<_> = event.uploads.iter().map(|view| view.state).collect();
            (states, event.removed)
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (vec![UploadState::Queued], vec![]),
            (vec![UploadState::Failed], vec![]),
            (vec![], vec![id]),
        ]
    );
}
