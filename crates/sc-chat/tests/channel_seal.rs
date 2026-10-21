mod origin;

use std::sync::Arc;

use origin::{FixedClock, NOW, Origin};
use sc_chat::channel::{BUSY_RETRY_MAX_MS, BUSY_RETRY_MS, Caller, Channel, InnerCall};
use sc_chat::crypto::DeviceKeys;
use sc_chat::sc_chat_wire::sign::verify_request;
use sc_chat::{ChatError, Tier, TransportResponse};
use uuid::Uuid;

const SESSION: &str = "0192e5c4-7b7e-7a3c-9d55-2b8f3c1e4a10";

fn sync_call() -> InnerCall {
    InnerCall {
        method: "POST".into(),
        path: "/chat/sync".into(),
        query: String::new(),
        body: br#"{"pos":0}"#.to_vec(),
    }
}

fn caller(keys: &DeviceKeys) -> Caller<'_> {
    Caller {
        session_id: SESSION,
        device_id: Uuid::from_u128(7),
        keys,
    }
}

fn outer(status: u16, retry_after_ms: Option<u64>) -> TransportResponse {
    TransportResponse {
        status,
        body: Vec::new(),
        retry_after_ms,
    }
}

#[test]
fn the_origin_opens_a_signed_inner_request() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Relay)
        .unwrap();
    let received = origin.receive(&sealed.envelope);
    let inner = &received.inner;
    assert_eq!(received.ock_v, 1);
    assert_eq!(received.enc, sealed.enc);
    assert_eq!(inner.ts_ms, NOW as u64);
    assert_eq!(inner.session_id.as_deref(), Some(SESSION));
    assert_eq!(
        inner.device_id.as_deref(),
        Some("00000000-0000-0000-0000-000000000007")
    );
    assert_eq!(
        (inner.method.as_str(), inner.path.as_str()),
        ("POST", "/chat/sync")
    );
    assert_eq!(inner.body, br#"{"pos":0}"#);
    assert_eq!(inner.tier, i32::from(Tier::Relay.wire()));
    assert_eq!(inner.max_bytes, Tier::Relay.max_bytes());
    assert!(inner.bind.is_none());
    verify_request(&keys.auth_public(), inner, &received.enc).unwrap();
}

#[test]
fn a_sealed_answer_opens_and_a_tampered_one_does_not() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    let response = origin.receive(&sealed.envelope).answer(200, r#"{"pos":4}"#);
    let answer = channel.receive(&sealed, &response).unwrap();
    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, br#"{"pos":4}"#);
    assert_eq!(answer.header("content-type"), Some("application/json"));

    for at in [0, 30, response.body.len() - 1] {
        let mut tampered = response.clone();
        tampered.body[at] ^= 0x40;
        assert_eq!(
            channel.receive(&sealed, &tampered).err(),
            Some(ChatError::Crypto)
        );
    }
    let other = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    assert_eq!(
        channel.receive(&other, &response).err(),
        Some(ChatError::Crypto)
    );
}

#[test]
fn every_request_takes_fresh_randomness() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let first = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    let second = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    assert_ne!(first.enc, second.enc);
    assert_ne!(first.envelope, second.envelope);
    assert_ne!(
        first.response_key.as_bytes(),
        second.response_key.as_bytes()
    );
}

#[test]
fn a_clock_answer_moves_the_request_clock_to_the_server() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    let server_now = NOW + 300_000;
    let body = format!(r#"{{"error":"chat_clock","server_now":{server_now}}}"#);
    let response = origin.receive(&sealed.envelope).answer(401, &body);
    let answer = channel.receive(&sealed, &response).unwrap();
    assert_eq!(answer.error(), Some(ChatError::Clock { server_now }));
    assert_eq!(channel.offset_ms(), 300_000);
    let retried = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    assert_eq!(
        origin.receive(&retried.envelope).inner.ts_ms,
        server_now as u64
    );
}

#[test]
fn a_clock_answer_without_server_time_keeps_the_clock() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    let response = origin
        .receive(&sealed.envelope)
        .answer(401, r#"{"error":"chat_clock"}"#);
    let answer = channel.receive(&sealed, &response).unwrap();
    assert_eq!(answer.error(), Some(ChatError::Clock { server_now: 0 }));
    assert_eq!(channel.offset_ms(), 0);
}

#[test]
fn outer_statuses_map_to_errors_without_touching_the_clock() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Pro)
        .unwrap();
    let cases = [
        (outer(503, Some(2_500)), ChatError::Busy { retry_ms: 2_500 }),
        (
            outer(503, None),
            ChatError::Busy {
                retry_ms: BUSY_RETRY_MS,
            },
        ),
        (
            outer(503, Some(0)),
            ChatError::Busy {
                retry_ms: BUSY_RETRY_MS,
            },
        ),
        (
            outer(503, Some(u64::MAX)),
            ChatError::Busy {
                retry_ms: BUSY_RETRY_MAX_MS,
            },
        ),
        (outer(400, None), ChatError::Protocol),
        (outer(413, None), ChatError::Invalid),
        (outer(502, None), ChatError::Offline),
        (outer(200, None), ChatError::Crypto),
    ];
    for (response, expected) in cases {
        assert_eq!(channel.receive(&sealed, &response).err(), Some(expected));
    }
    assert_eq!(channel.offset_ms(), 0);
}

#[test]
fn the_channel_seals_to_the_ock_it_was_told_to_use() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    assert!(!channel.use_ock(3));
    assert_eq!(channel.ock_version(), 1);
    assert!(channel.use_ock(2));
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    assert_eq!(sealed.envelope[1], 2);
    assert_eq!(origin.receive(&sealed.envelope).ock_v, 2);
}

#[test]
fn a_channel_without_pins_refuses_to_seal() {
    let channel = Channel::with_pins(Arc::new(FixedClock(NOW)), Vec::new());
    let keys = DeviceKeys::generate().unwrap();
    assert_eq!(
        channel
            .seal(&sync_call(), &caller(&keys), Tier::Direct)
            .err(),
        Some(ChatError::Crypto)
    );
}

#[test]
fn the_shipped_channel_seals_to_the_first_published_pin() {
    let channel = Channel::new(Arc::new(FixedClock(NOW)));
    let keys = DeviceKeys::generate().unwrap();
    let sealed = channel
        .seal(&sync_call(), &caller(&keys), Tier::Direct)
        .unwrap();
    assert_eq!(sealed.envelope[0], 1);
    assert_eq!(
        sealed.envelope[1],
        sc_chat::sc_chat_wire::ock::OCK_PINS[0].0
    );
}

#[test]
fn a_field_with_a_newline_is_never_signed() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let smuggled = InnerCall {
        query: "a=1\nb=2".into(),
        ..sync_call()
    };
    assert_eq!(
        channel.seal(&smuggled, &caller(&keys), Tier::Direct).err(),
        Some(ChatError::Invalid)
    );
    let bad_session = Caller {
        session_id: "x\ny",
        ..caller(&keys)
    };
    assert_eq!(
        channel.seal(&sync_call(), &bad_session, Tier::Direct).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn an_oversized_call_does_not_fit_the_envelope() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let huge = InnerCall {
        body: vec![b'x'; 64 * 1024],
        ..sync_call()
    };
    assert_eq!(
        channel.seal(&huge, &caller(&keys), Tier::Direct).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn a_dev_ock_pin_reads_as_version_and_hex_key() {
    let key = "dde332903df0f9e9fbc91a814131b5bcea28b46f5728e2a7e28700ed4a6c2d4d";
    let (version, public) = sc_chat::channel::dev_pin(&format!(" 1:{key}\n")).unwrap();
    assert_eq!(version, 1);
    assert_eq!((public[0], public[31]), (0xdd, 0x4d));
    for broken in [
        "",
        key,
        "0:00",
        &format!("0:{key}"),
        &format!("x:{key}"),
        &format!("1:{}", &key[2..]),
        &format!("1:{}zz", &key[2..]),
    ] {
        assert_eq!(sc_chat::channel::dev_pin(broken), None, "{broken}");
    }
}
