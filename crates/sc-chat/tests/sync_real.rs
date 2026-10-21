mod sync_support;

use std::sync::Arc;

use sc_chat::api::wire::b64;
use sc_chat::api::{Api, Identity};
use sc_chat::channel::Channel;
use sc_chat::crypto::{self, Ck, DeviceKeys, MessageAt};
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::store::{Store, SyncState};
use sc_chat::sync::{SyncEngine, SyncOutcome};
use sc_chat::{ChatError, HistoryQuery, MessageBody, MessageClass, Tier};
use serde_json::{Value, json};
use sync_support::fixtures::{answer, entry, summary, text};
use sync_support::{DEVICE, ME, PEER, PEER_DEVICE, Recorder, START_MS, Server, Step, TestClock};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);
const CK: [u8; 32] = [7; 32];

struct Real {
    server: Arc<Server>,
    events: Arc<Recorder>,
    keys: Arc<DeviceKeys>,
    engine: SyncEngine,
}

fn real() -> Real {
    let clock = Arc::new(TestClock::at(START_MS));
    let server = Arc::new(Server::new(&[Tier::Direct, Tier::Relay], clock.clone()));
    let api = Arc::new(Api::with_exchange(
        Arc::new(Channel::new(clock)),
        server.clone(),
        server.clone(),
    ));
    let keys = Arc::new(DeviceKeys::generate().unwrap());
    api.set_identity(Some(Identity {
        session_id: "session".into(),
        device_id: DEVICE,
        keys: keys.clone(),
    }));
    let store = Store::open_in_memory(&[9; 32]).unwrap();
    store.set_me(ME).unwrap();
    store
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
    store.put_conversations(&[summary(A, 0)]).unwrap();
    let events = Arc::new(Recorder::default());
    let engine = SyncEngine::new(api, Arc::new(store), events.clone());
    engine.set_user(Some(ME.into()));
    Real {
        server,
        events,
        keys,
        engine,
    }
}

impl Real {
    fn sealed_key(&self) -> Value {
        let sealed = sc_chat::sc_chat_wire::ck::seal_for_device(
            &self.keys.box_public().unwrap(),
            A.as_bytes(),
            1,
            DEVICE.as_bytes(),
            &CK,
            [5; 32],
        )
        .unwrap();
        json!([{"epoch": 1, "sealed": b64::encode(&sealed)}])
    }

    fn texts(&self) -> Vec<(Option<i64>, bool, String)> {
        let query = HistoryQuery {
            before: None,
            after: None,
            limit: 50,
        };
        self.engine
            .store()
            .messages(A, &query)
            .unwrap()
            .messages
            .into_iter()
            .map(|message| match message.body {
                MessageBody::Text(rich) => (message.seq, message.mine, rich.text),
                other => (message.seq, message.mine, format!("{other:?}")),
            })
            .collect()
    }
}

fn row(seq: i64, client_id: Uuid, sender: &str, device: Uuid, body: &[u8]) -> Value {
    json!({
        "seq": seq,
        "client_id": client_id,
        "sender": sender,
        "sender_device": device,
        "class": "post",
        "epoch": 1,
        "target_seq": null,
        "body": b64::encode(body),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

fn peer_post(seq: i64, body: &str) -> Value {
    let client_id = Uuid::from_u128(0x500 + seq as u128);
    let at = MessageAt {
        conversation_id: A,
        epoch: 1,
        sender_device: PEER_DEVICE,
        client_id,
        class: MessageClass::Post,
    };
    let sealed = crypto::seal_message(&Ck::from_bytes(CK), &at, &text(body)).unwrap();
    row(seq, client_id, PEER, PEER_DEVICE, &sealed)
}

fn plan() -> sc_chat::sync::PollPlan {
    sc_chat::sync::PollPlan {
        tier: Tier::Direct,
        hold_ms: 0,
        delay_ms: 0,
    }
}

#[tokio::test]
async fn a_sealed_post_opens_into_the_store_and_a_repeat_stays_one_message() {
    let rig = real();
    let keys_path = format!("/chat/conversations/{A}/keys");
    rig.server.json("GET", &keys_path, rig.sealed_key());
    let post = peer_post(1, "hello");
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![post.clone()])]),
    );
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(3, INC, vec![entry(A, 1, vec![post])]),
    );

    assert_eq!(rig.engine.step(plan()).await, Ok(SyncOutcome::Changed));
    assert_eq!(rig.engine.step(plan()).await, Ok(SyncOutcome::Changed));

    assert_eq!(rig.texts(), vec![(Some(1), false, "hello".to_owned())]);
    let summary = rig.engine.store().conversation(A).unwrap().unwrap();
    assert_eq!((summary.last_seq, summary.unread), (1, 1));
    assert_eq!(rig.engine.status().unread, 1);
    assert_eq!(rig.server.calls_to("GET", &keys_path).len(), 1);
    let shown = &rig.events.messages()[0].messages[0];
    assert!(matches!(&shown.body, MessageBody::Text(rich) if rich.text == "hello"));
}

#[tokio::test]
async fn a_lost_send_is_replayed_and_its_echo_stays_one_message() {
    let rig = real();
    let client_id = Uuid::from_u128(0xc1);
    let outbox = Outbox::new(rig.engine.store().clone());
    outbox
        .push(OutboxItem {
            client_id,
            conversation_id: A,
            class: MessageClass::Post,
            target_seq: None,
            envelope: text("hi").encode_to_vec(),
            created_at: START_MS,
            attempts: 0,
            next_at: START_MS,
        })
        .unwrap();
    let path = format!("/chat/conversations/{A}/messages");
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        rig.sealed_key(),
    );
    rig.server.on("POST", &path, Step::Fail(ChatError::Offline));
    rig.server.json(
        "POST",
        &path,
        json!({"seq": 1, "ts": START_MS + 1000, "expires_at": null}),
    );

    assert_eq!(rig.engine.flush_outbox().await, Ok(1));

    let sends = rig.server.calls_to("POST", &path);
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[0].body["client_id"], sends[1].body["client_id"]);
    assert_eq!((sends[0].tier, sends[1].tier), (Tier::Direct, Tier::Relay));
    let body = b64::decode(sends[1].body["body"].as_str().unwrap()).unwrap();
    let echo = row(1, client_id, ME, DEVICE, &body);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![echo])]),
    );

    rig.engine.step(plan()).await.unwrap();

    assert_eq!(rig.texts(), vec![(Some(1), true, "hi".to_owned())]);
    assert!(outbox.due(START_MS + 60_000, 10).unwrap().is_empty());
    let summary = rig.engine.store().conversation(A).unwrap().unwrap();
    assert_eq!((summary.last_seq, summary.unread), (1, 0));
}

fn deleted_marker(seq: i64, target: i64) -> Value {
    json!({
        "seq": seq,
        "client_id": Uuid::from_u128(0x900 + seq as u128),
        "sender": PEER,
        "sender_device": null,
        "class": "system",
        "epoch": 1,
        "target_seq": target,
        "body": b64::encode(&serde_json::to_vec(&json!({"kind": "deleted", "actor": PEER})).unwrap()),
        "ts": START_MS + seq * 1000,
        "expires_at": null,
    })
}

#[tokio::test]
async fn a_delete_for_everyone_from_another_device_scrubs_the_target() {
    let rig = real();
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        rig.sealed_key(),
    );
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![peer_post(1, "secret plan")])]),
    );
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(3, INC, vec![entry(A, 2, vec![deleted_marker(2, 1)])]),
    );

    assert_eq!(rig.engine.step(plan()).await, Ok(SyncOutcome::Changed));
    assert_eq!(rig.engine.step(plan()).await, Ok(SyncOutcome::Changed));

    let texts = rig.texts();
    assert_eq!(texts[0], (Some(1), false, "Deleted".to_owned()));
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}"))
            .is_empty()
    );
    let hits = sc_chat::search::Search::new(rig.engine.store().clone())
        .query(&sc_chat::SearchQuery {
            text: "secret".into(),
            conversation_id: None,
            limit: 10,
        })
        .unwrap();
    assert!(hits.is_empty());
    let shown = rig.events.messages().pop().unwrap();
    assert!(
        shown
            .messages
            .iter()
            .any(|message| message.seq == Some(1) && message.body == MessageBody::Deleted)
    );
    let summary = rig.engine.store().conversation(A).unwrap().unwrap();
    assert_eq!(
        summary.preview.map(|preview| preview.kind),
        Some(sc_chat::PreviewKind::Deleted)
    );
}
