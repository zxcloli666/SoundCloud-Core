mod origin;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use origin::{NOW, Origin, Received};
use sc_chat::api::{Api, BindSigner, Identity, Signer};
use sc_chat::channel::BUSY_RETRY_MAX_MS;
use sc_chat::crypto::DeviceKeys;
use sc_chat::sc_chat_wire::bind::verify_bind;
use sc_chat::sc_chat_wire::sign::verify_request;
use sc_chat::{
    ChatError, Tier, Transport, TransportError, TransportRequest, TransportResponse, async_trait,
};
use serde_json::{Value, json};
use uuid::Uuid;

const SESSION: &str = "0192e5c4-7b7e-7a3c-9d55-2b8f3c1e4a10";
const DEVICE: Uuid = Uuid::from_u128(0xd1);

enum Reply {
    Inner(u32, &'static str),
    Outer(u16, Option<u64>),
    Garbage,
}

struct Wire {
    origin: Origin,
    script: Mutex<VecDeque<Reply>>,
    seen: Mutex<Vec<(Tier, Received)>>,
}

#[async_trait]
impl Transport for Wire {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct, Tier::Relay]
    }

    async fn exchange(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let received = self.origin.receive(&request.envelope);
        let reply = self.script.lock().unwrap().pop_front();
        let response = match reply {
            Some(Reply::Inner(status, body)) => received.answer(status, body),
            Some(Reply::Outer(status, retry_after_ms)) => TransportResponse {
                status,
                body: Vec::new(),
                retry_after_ms,
            },
            Some(Reply::Garbage) => TransportResponse {
                status: 200,
                body: b"<html>blocked</html>".to_vec(),
                retry_after_ms: None,
            },
            None => return Err(TransportError::Unreachable),
        };
        self.seen.lock().unwrap().push((request.tier, received));
        Ok(response)
    }
}

struct Sealed {
    wire: Arc<Wire>,
    api: Api,
    keys: Arc<DeviceKeys>,
}

fn sealed(replies: Vec<Reply>) -> Sealed {
    let origin = Origin::new();
    let channel = Arc::new(origin.channel());
    let wire = Arc::new(Wire {
        origin,
        script: Mutex::new(replies.into()),
        seen: Mutex::new(Vec::new()),
    });
    let api = Api::new(channel, wire.clone());
    let keys = Arc::new(DeviceKeys::generate().unwrap());
    api.set_identity(Some(Identity {
        session_id: SESSION.into(),
        device_id: DEVICE,
        keys: keys.clone(),
    }));
    Sealed { wire, api, keys }
}

impl Sealed {
    fn tiers(&self) -> Vec<Tier> {
        self.wire
            .seen
            .lock()
            .unwrap()
            .iter()
            .map(|(tier, _)| *tier)
            .collect()
    }
}

#[tokio::test]
async fn a_device_call_is_sealed_signed_and_opened() {
    let rig = sealed(vec![Reply::Inner(200, "[]")]);

    assert_eq!(rig.api.devices().await, Ok(Vec::new()));

    let seen = rig.wire.seen.lock().unwrap();
    let (tier, received) = &seen[0];
    let inner = &received.inner;
    assert_eq!((*tier, received.ock_v), (Tier::Direct, 1));
    assert_eq!(
        (inner.method.as_str(), inner.path.as_str()),
        ("GET", "/chat/devices")
    );
    assert_eq!(inner.session_id.as_deref(), Some(SESSION));
    assert_eq!(inner.device_id, Some(DEVICE.to_string()));
    assert_eq!(inner.ts_ms, NOW as u64);
    verify_request(&rig.keys.auth_public(), inner, &received.enc).unwrap();
}

#[tokio::test]
async fn a_login_call_carries_a_bind_and_is_signed_by_it() {
    let rig = sealed(vec![Reply::Inner(200, r#"{"status":"pending"}"#)]);
    let signer = Signer::Bind(BindSigner {
        device_id: DEVICE,
        keys: rig.keys.clone(),
        target_session: Some(SESSION.into()),
    });

    let answer: Value = rig
        .api
        .json_as(&signer, "GET", "/auth/login/status", "id=x", None::<&()>)
        .await
        .unwrap();

    assert_eq!(answer, json!({"status": "pending"}));
    let seen = rig.wire.seen.lock().unwrap();
    let received = &seen[0].1;
    let bind = received.inner.bind.as_ref().unwrap();
    verify_bind(bind).unwrap();
    verify_request(&bind.auth_pub, &received.inner, &received.enc).unwrap();
    assert_eq!(bind.device_id, DEVICE.to_string());
    assert_eq!(received.inner.device_id, None);
    assert_eq!(received.inner.session_id.as_deref(), Some(SESSION));
}

#[tokio::test]
async fn an_answer_that_does_not_open_moves_to_the_next_tier() {
    let rig = sealed(vec![Reply::Garbage, Reply::Inner(200, "[]")]);

    assert_eq!(rig.api.devices().await, Ok(Vec::new()));
    assert_eq!(rig.tiers(), vec![Tier::Direct, Tier::Relay]);
    assert_eq!(rig.api.tier_order()[0], Tier::Relay);
}

#[tokio::test]
async fn a_relay_busy_without_retry_after_moves_on_and_a_long_one_is_capped() {
    let rig = sealed(vec![
        Reply::Outer(503, None),
        Reply::Inner(200, "[]"),
        Reply::Outer(503, Some(86_400_000)),
    ]);

    assert_eq!(rig.api.devices().await, Ok(Vec::new()));
    assert_eq!(
        rig.api.devices().await,
        Err(ChatError::Busy {
            retry_ms: BUSY_RETRY_MAX_MS
        })
    );
    assert_eq!(rig.tiers(), vec![Tier::Direct, Tier::Relay, Tier::Relay]);
}

#[tokio::test]
async fn a_clock_answer_moves_the_clock_and_is_sent_once_more() {
    let rig = sealed(vec![
        Reply::Inner(400, r#"{"error":"chat_clock","server_now":1760958600000}"#),
        Reply::Inner(200, "[]"),
    ]);

    assert_eq!(rig.api.devices().await, Ok(Vec::new()));

    assert_eq!(rig.api.channel().now_ms(), 1_760_958_600_000);
    let seen = rig.wire.seen.lock().unwrap();
    assert_eq!(seen[1].1.inner.ts_ms, 1_760_958_600_000);
}

#[tokio::test]
async fn a_clock_answer_without_server_time_keeps_the_clock_and_is_not_resent() {
    let rig = sealed(vec![Reply::Inner(400, r#"{"error":"chat_clock"}"#)]);

    assert_eq!(
        rig.api.devices().await,
        Err(ChatError::Clock { server_now: 0 })
    );

    assert_eq!(rig.api.channel().now_ms(), NOW);
    assert_eq!(rig.tiers().len(), 1);
}
