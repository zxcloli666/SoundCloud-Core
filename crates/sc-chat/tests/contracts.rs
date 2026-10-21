use std::sync::Arc;

use sc_chat::api::Api;
use sc_chat::channel::Channel;
use sc_chat::store::Store;
use sc_chat::sync::SyncEngine;
use sc_chat::{
    ChatError, ChatStatus, Clock, CoreEvent, DeleteScope, EmbedKind, EmbedsQuery, Events,
    GoneEvent, GoneMessage, HIDDEN_SYNC_EVERY_MS, MessageBody, MessageClass, MessageTarget, Mute,
    RichText, RoomModeration, Tier, Transport, TransportError, TransportRequest, TransportResponse,
    async_trait, is_disappearing_ttl,
};
use serde_json::{Value, json};
use uuid::Uuid;

struct FixedClock;

#[async_trait]
impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        1_000
    }

    async fn sleep(&self, _ms: u64) {}
}

struct NoTransport;

#[async_trait]
impl Transport for NoTransport {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct, Tier::Relay, Tier::Pro]
    }

    async fn exchange(
        &self,
        _request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        Err(TransportError::Unreachable)
    }
}

struct NoEvents;

impl Events for NoEvents {
    fn emit(&self, _event: CoreEvent) {}
}

fn engine() -> SyncEngine {
    let channel = Arc::new(Channel::new(Arc::new(FixedClock)));
    let api = Arc::new(Api::new(channel, Arc::new(NoTransport)));
    let store = Arc::new(Store::open_in_memory(&[7; 32]).unwrap());
    SyncEngine::new(api, store, Arc::new(NoEvents))
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn hidden_engine_syncs_once_a_minute_without_holding() {
    let engine = engine();
    let plan = engine.plan(Tier::Relay);
    assert_eq!(plan.hold_ms, 0);
    assert_eq!(plan.delay_ms, HIDDEN_SYNC_EVERY_MS);
}

#[test]
fn visible_engine_parks_a_long_poll_at_once() {
    let engine = engine();
    assert!(engine.set_visible(true));
    assert!(!engine.set_visible(true));
    assert_eq!(engine.plan(Tier::Direct).hold_ms, 20_000);
    assert_eq!(engine.plan(Tier::Pro).hold_ms, 15_000);
    assert_eq!(engine.plan(Tier::Direct).delay_ms, 0);
}

#[test]
fn channel_follows_the_server_clock() {
    let channel = Channel::new(Arc::new(FixedClock));
    channel.note_server_now(61_000);
    assert_eq!(channel.now_ms(), 61_000);
    assert!(channel.use_ock(2));
    assert!(!channel.use_ock(9));
    assert_eq!(channel.ock_version(), 2);
}

#[test]
fn server_errors_map_to_codes() {
    let busy = ChatError::from_server(&json!({"error": "chat_busy", "retry_ms": 1500}));
    assert_eq!(busy, ChatError::Busy { retry_ms: 1500 });
    assert_eq!(busy.retry_ms(), Some(1500));
    let clock = ChatError::from_server(&json!({"error": "chat_clock", "server_now": 42}));
    assert_eq!(clock, ChatError::Clock { server_now: 42 });
    let odd = ChatError::from_server(&json!({"error": "nope"}));
    assert_eq!(odd, ChatError::Protocol);
}

#[test]
fn errors_reach_the_webview_tagged_by_code() {
    assert_eq!(
        to_json(&ChatError::SlowMode { retry_ms: 5 }),
        json!({"code": "slow_mode", "retryMs": 5})
    );
    assert_eq!(
        to_json(&ChatError::SessionBoundOther),
        json!({"code": "session_bound_other"})
    );
}

#[test]
fn tagged_inputs_keep_their_shape() {
    let id = Uuid::nil();
    assert_eq!(to_json(&MessageTarget::Seq(4)), json!({"seq": 4}));
    assert_eq!(
        to_json(&MessageTarget::Pending(id)),
        json!({"pending": id.to_string()})
    );
    assert_eq!(
        to_json(&Mute::Until { at: 9 }),
        json!({"kind": "until", "at": 9})
    );
    assert_eq!(
        to_json(&RoomModeration::Mute {
            user_id: "5".into(),
            seconds: 300
        }),
        json!({"action": "mute", "userId": "5", "seconds": 300})
    );
    assert_eq!(to_json(&DeleteScope::Everyone), json!("everyone"));
}

#[test]
fn message_bodies_are_tagged_by_type() {
    let body = MessageBody::Text(RichText {
        text: "hi".into(),
        spans: vec![],
    });
    assert_eq!(
        to_json(&body),
        json!({"type": "text", "text": "hi", "spans": []})
    );
    assert_eq!(to_json(&MessageBody::Locked), json!({"type": "locked"}));
}

#[test]
fn gone_events_carry_conversations_and_messages() {
    let id = Uuid::nil();
    let event = GoneEvent {
        conversations: vec![],
        messages: vec![GoneMessage {
            conversation_id: id,
            seq: 3,
        }],
    };
    assert_eq!(
        to_json(&event),
        json!({"conversations": [], "messages": [{"conversationId": id.to_string(), "seq": 3}]})
    );
}

#[test]
fn status_starts_off() {
    let status = to_json(&ChatStatus::off());
    assert_eq!(status["phase"], json!("off"));
    assert_eq!(status["keyStorage"], json!("os"));
}

#[test]
fn classes_match_the_wire() {
    for class in [
        MessageClass::Post,
        MessageClass::Edit,
        MessageClass::React,
        MessageClass::Pin,
        MessageClass::Meta,
        MessageClass::System,
    ] {
        assert_eq!(MessageClass::parse(class.as_str()), Some(class));
        assert_eq!(to_json(&class), json!(class.as_str()));
    }
}

#[test]
fn only_planned_timers_disappear() {
    assert!(is_disappearing_ttl(3_600));
    assert!(is_disappearing_ttl(604_800));
    assert!(!is_disappearing_ttl(60));
    assert_eq!(Tier::Pro.max_bytes(), 24 * 1024);
    assert_eq!(Tier::Relay.max_bytes(), 256 * 1024);
}

#[test]
fn embed_queries_filter_by_kind() {
    let query: EmbedsQuery =
        serde_json::from_value(json!({"kinds": ["track", "room_invite"], "limit": 50})).unwrap();
    assert_eq!(query.kinds, vec![EmbedKind::Track, EmbedKind::RoomInvite]);
    assert_eq!(query.before, None);
}
