mod origin;

use origin::{NOW, Origin};
use sc_chat::channel::{InnerCall, Sealed};
use sc_chat::crypto::DeviceKeys;
use sc_chat::login::{LOGIN_MAX_BYTES, LOGIN_PATHS, is_login_path, seal_login};
use sc_chat::sc_chat_wire::bind::verify_bind;
use sc_chat::sc_chat_wire::sign::verify_request;
use sc_chat::{ChatError, Tier};
use uuid::Uuid;

fn call(path: &str) -> InnerCall {
    InnerCall {
        method: "POST".into(),
        path: path.into(),
        query: String::new(),
        body: br#"{"mode":"pull"}"#.to_vec(),
    }
}

#[test]
fn a_sealed_login_carries_a_bind_signed_by_the_device() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let device_id = Uuid::new_v4();
    let sealed = seal_login(
        &channel,
        &keys,
        device_id,
        &call("/auth/link/create"),
        None,
        Tier::Relay,
    )
    .unwrap();
    let received = origin.receive(&sealed.envelope);
    let inner = &received.inner;
    let bind = inner.bind.as_ref().expect("a login carries a bind");
    verify_bind(bind).unwrap();
    assert_eq!(bind.device_id, device_id.to_string());
    assert_eq!(bind.auth_pub, keys.auth_public());
    assert_eq!(bind.box_pub, keys.box_public().unwrap());
    assert_eq!(bind.ts_ms, NOW as u64);
    assert_eq!(inner.ts_ms, NOW as u64);
    assert!(inner.session_id.is_none() && inner.device_id.is_none());
    assert_eq!(inner.max_bytes, LOGIN_MAX_BYTES);
    assert_eq!(received.ock_v, 1);
    verify_request(&bind.auth_pub, inner, &received.enc).unwrap();

    let response = received.answer(200, r#"{"loginRequestId":"r1"}"#);
    let answer = channel.receive(&sealed, &response).unwrap();
    assert_eq!(answer.body, br#"{"loginRequestId":"r1"}"#);
}

#[test]
fn a_relogin_names_the_session_it_binds() {
    let origin = Origin::new();
    let channel = origin.channel();
    let keys = DeviceKeys::generate().unwrap();
    let session = "0192e5c4-7b7e-7a3c-9d55-2b8f3c1e4a10";
    let sealed = seal_login(
        &channel,
        &keys,
        Uuid::new_v4(),
        &call("/auth/login"),
        Some(session),
        Tier::Pro,
    )
    .unwrap();
    let inner = origin.receive(&sealed.envelope).inner;
    assert_eq!(inner.session_id.as_deref(), Some(session));
    assert_eq!(inner.max_bytes, Tier::Pro.max_bytes());
}

fn seal_path(origin: &Origin, path: &str) -> Result<Sealed, ChatError> {
    let keys = DeviceKeys::generate().unwrap();
    let channel = origin.channel();
    seal_login(
        &channel,
        &keys,
        Uuid::new_v4(),
        &call(path),
        None,
        Tier::Direct,
    )
}

#[test]
fn only_the_sealed_auth_paths_take_a_bind() {
    let origin = Origin::new();
    for path in LOGIN_PATHS {
        assert!(is_login_path(path));
        assert!(seal_path(&origin, path).is_ok(), "{path}");
    }
    for path in ["/chat/sync", "/auth/session", "/auth/login/"] {
        assert!(!is_login_path(path));
        assert_eq!(
            seal_path(&origin, path).err(),
            Some(ChatError::Invalid),
            "{path}"
        );
    }
}
