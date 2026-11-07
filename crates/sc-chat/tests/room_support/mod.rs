#![allow(dead_code)]

use sc_chat::api::wire::b64;
use sc_chat::crypto::{self, Ck, MessageAt};
use sc_chat::envelope::{self, Content};
use sc_chat::sc_chat_wire::proto::Attachment;
use sc_chat::{ConversationMeta, MessageClass};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::store_support::avatar;
use crate::sync_support::fixtures::{answer, entry, view};
use crate::sync_support::real::{CK, INC, Real, direct_plan};
use crate::sync_support::{PEER, PEER_DEVICE, START_MS};

pub const R: Uuid = Uuid::from_u128(0x7e);
pub const X: u128 = 0x61;
pub const Y: u128 = 0x62;

pub fn backdrop(n: u128) -> Attachment {
    let mut attachment = avatar(n);
    attachment.size = 409_600;
    attachment.w = 1920;
    attachment.h = 1080;
    attachment
}

pub fn room_meta(n: Option<u128>) -> ConversationMeta {
    ConversationMeta {
        avatar: n.map(backdrop),
        ..ConversationMeta::default()
    }
}

pub fn room_view(meta: Option<&ConversationMeta>, epoch: u32) -> Value {
    let mut view = view(R, 0);
    view["kind"] = json!("room");
    view["peer"] = Value::Null;
    if let Some(meta) = meta {
        let envelope = envelope::encode(&Content::Meta(Box::new(meta.clone()))).unwrap();
        let meta_ct = crypto::seal_meta(&Ck::from_bytes(CK), R, epoch, &envelope).unwrap();
        view["meta_ct"] = json!(b64::encode(&meta_ct));
        view["meta_epoch"] = json!(epoch);
    }
    view
}

pub fn meta_message(seq: i64, meta: &ConversationMeta) -> Value {
    let client_id = Uuid::from_u128(0x900 + seq as u128);
    let at = MessageAt {
        conversation_id: R,
        epoch: 1,
        sender_device: PEER_DEVICE,
        client_id,
        class: MessageClass::Meta,
    };
    let envelope = envelope::encode(&Content::Meta(Box::new(meta.clone()))).unwrap();
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

pub fn conversation_path() -> String {
    format!("/chat/conversations/{R}")
}

pub fn conversation_gets(rig: &Real) -> usize {
    rig.server.calls_to("GET", &conversation_path()).len()
}

pub fn keys_gets(rig: &Real) -> usize {
    rig.server
        .calls_to("GET", &format!("/chat/conversations/{R}/keys"))
        .len()
}

pub async fn joined(rig: &Real, view: Value) {
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{R}/messages"),
        json!({"messages": [], "more": false}),
    );
    rig.server.json("GET", &conversation_path(), view);
    rig.engine.adopt_room(R, "ROOM42", 4).await.unwrap();
}

pub async fn live(rig: &Real, pos: u64, last_seq: i64, messages: Vec<Value>) {
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(pos, INC, vec![entry(R, last_seq, messages)]),
    );
    rig.engine.step(direct_plan()).await.unwrap();
}

pub fn resolves(rig: &Real, n: u128) -> Option<Attachment> {
    rig.engine
        .store()
        .attachment(R, Uuid::from_u128(n))
        .unwrap()
}

pub fn carried_backdrop(rig: &Real) -> Option<Uuid> {
    let store = rig.engine.store();
    assert_eq!(
        store.conversation(R).unwrap().unwrap().avatar_media_id,
        None
    );
    store
        .room_backdrop(R)
        .unwrap()
        .map(|image| Uuid::parse_str(&image.media_id).unwrap())
}
