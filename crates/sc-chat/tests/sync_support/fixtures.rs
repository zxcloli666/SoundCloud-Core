use sc_chat::api::wire::b64;
use sc_chat::sc_chat_wire::proto::{Envelope, Text, envelope::Body};
use sc_chat::{
    ConvId, ConversationKind, ConversationSummary, MemberRole, MemberState, NotifyLevel,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::cipher::{seal_body, sealed_ck};
use super::{ME, PEER, PEER_DEVICE, START_MS};

pub fn text(body: &str) -> Envelope {
    Envelope {
        v: 1,
        body: Some(Body::Text(Text {
            text: body.into(),
            spans: Vec::new(),
        })),
        ..Envelope::default()
    }
}

pub fn post(conversation_id: ConvId, seq: i64, epoch: u32, body: &str) -> Value {
    post_from(conversation_id, seq, epoch, PEER, body)
}

pub fn post_from(conversation_id: ConvId, seq: i64, epoch: u32, sender: &str, body: &str) -> Value {
    json!({
        "seq": seq,
        "client_id": Uuid::from_u128(seq as u128 + 0x1000),
        "sender": sender,
        "sender_device": PEER_DEVICE,
        "class": "post",
        "epoch": epoch,
        "target_seq": null,
        "body": b64::encode(&seal_body(conversation_id, epoch, &text(body))),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

pub fn system(seq: i64, body: Value) -> Value {
    json!({
        "seq": seq,
        "client_id": Uuid::from_u128(seq as u128 + 0x2000),
        "sender": PEER,
        "sender_device": null,
        "class": "system",
        "epoch": 0,
        "target_seq": null,
        "body": b64::encode(&serde_json::to_vec(&body).unwrap()),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

pub fn entry(id: ConvId, last_seq: i64, messages: Vec<Value>) -> Value {
    json!({
        "id": id,
        "last_seq": last_seq,
        "key_epoch": 1,
        "messages": messages,
        "more": false,
        "marks": [],
        "typing": [],
    })
}

pub fn answer(pos: u64, inc: Uuid, convs: Vec<Value>) -> Value {
    json!({
        "pos": pos,
        "inc": inc,
        "reset": false,
        "convs": convs,
        "gone": [],
        "gone_msgs": [],
        "devices": [],
        "requests_count": 0,
    })
}

pub fn view(id: ConvId, last_seq: i64) -> Value {
    json!({
        "id": id,
        "kind": "direct",
        "peer": PEER,
        "created_by": ME,
        "created_at": START_MS - 10_000,
        "last_seq": last_seq,
        "last_at": START_MS + last_seq * 1000,
        "key_epoch": 1,
        "meta_ct": null,
        "meta_epoch": null,
        "settings": {},
        "member_count": 2,
        "ttl_s": null,
        "room_ref": null,
        "me": {
            "role": "member",
            "state": "active",
            "accepted": true,
            "joined_seq": 0,
            "joined_epoch": 0,
            "last_read_seq": 0,
            "muted_until": null,
            "notify": "all",
            "pinned_at": null,
            "archived": false,
            "hidden_before_seq": 0,
            "invited_by": null,
        },
    })
}

pub fn card(user: &str, name: &str) -> Value {
    json!({
        "user_id": user,
        "username": name,
        "avatar_url": "https://i1.sndcdn.com/avatars-x.jpg",
        "permalink_url": format!("https://soundcloud.com/{name}"),
        "verified": false,
        "blocked": false,
        "contact": true,
        "can_message": "direct",
    })
}

pub fn keys(conversation_id: ConvId, epochs: &[u32]) -> Value {
    Value::Array(
        epochs
            .iter()
            .map(|epoch| {
                json!({
                    "epoch": epoch,
                    "sealed": b64::encode(&sealed_ck(conversation_id, *epoch)),
                })
            })
            .collect(),
    )
}

pub fn device(id: Uuid, created_at: i64, unlocked: bool, current: bool) -> Value {
    json!({
        "id": id,
        "platform": "macos",
        "name": "Mac",
        "app_version": "8.7.0",
        "tier": "direct",
        "status": "active",
        "created_at": created_at,
        "last_seen_at": created_at,
        "history_unlocked": unlocked,
        "current": current,
    })
}

pub fn summary(id: ConvId, last_seq: i64) -> ConversationSummary {
    ConversationSummary {
        id,
        kind: ConversationKind::Direct,
        title: None,
        avatar_url: None,
        peer: None,
        role: MemberRole::Member,
        state: MemberState::Active,
        accepted: true,
        member_count: 2,
        last_seq,
        last_at: START_MS + last_seq * 1000,
        last_read_seq: 0,
        unread: 0,
        mentions: 0,
        preview: None,
        muted_until: None,
        notify: NotifyLevel::All,
        pinned_at: None,
        archived: false,
        ttl_s: None,
        room_code: None,
        history_locked: false,
        has_draft: false,
    }
}
