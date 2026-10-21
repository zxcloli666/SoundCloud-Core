use sc_chat::api::inner_call;
use sc_chat::channel::Caller;
use sc_chat::{ChatError, Tier, Transport, TransportRequest};

use crate::support::{Client, Stack};

#[tokio::test]
#[ignore]
async fn a_captured_request_sent_again_is_refused_as_a_replay() {
    let stack = Stack::from_env();
    let client = Client::up(&stack, &stack.person("fay")).await;
    let call = inner_call("GET", "/chat/devices", "", None::<&()>).unwrap();
    let session = client.person.session_id.to_string();
    let caller = Caller {
        session_id: &session,
        device_id: client.person.device_id,
        keys: &client.person.keys,
    };
    let sealed = client
        .api
        .channel()
        .seal(&call, &caller, Tier::Direct)
        .unwrap();
    let request = || TransportRequest {
        tier: Tier::Direct,
        envelope: sealed.envelope.clone(),
        timeout_ms: 8_000,
        long_poll: false,
    };

    let first = client.transport.exchange(request()).await.unwrap();
    let first = client.api.channel().receive(&sealed, &first).unwrap();
    assert!(first.is_success());
    let again = client.transport.exchange(request()).await.unwrap();
    let refused = match client.api.channel().receive(&sealed, &again) {
        Ok(answer) => answer.error(),
        Err(error) => Some(error),
    };
    assert_eq!(refused, Some(ChatError::Replay));
}
