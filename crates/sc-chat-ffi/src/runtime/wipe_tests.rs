use std::sync::{Arc, Mutex, OnceLock, Weak};

use async_trait::async_trait;
use sc_chat::api::Identity;
use sc_chat::crypto::DeviceKeys;
use sc_chat::{Tier, TransportRequest, TransportResponse};

use super::Chat;
use crate::platform::{ChatTransport, NetFailure};
use crate::runtime::doubles::chat_on;

#[derive(Default)]
struct Watching {
    chat: OnceLock<Weak<Chat>>,
    logged_out_at_send: Mutex<Vec<bool>>,
}

#[async_trait]
impl ChatTransport for Watching {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct]
    }

    async fn exchange(&self, _request: TransportRequest) -> Result<TransportResponse, NetFailure> {
        if let Some(chat) = self.chat.get().and_then(Weak::upgrade) {
            let logged_out = chat.session.logged_out();
            self.logged_out_at_send.lock().unwrap().push(logged_out);
        }
        Err(NetFailure::Unreachable)
    }
}

#[tokio::test]
async fn log_out_marks_the_session_before_it_removes_the_device() {
    let transport = Arc::new(Watching::default());
    let (chat, _scratch) = chat_on(transport.clone());
    transport.chat.set(Arc::downgrade(&chat)).ok().unwrap();
    chat.session.replace(Some("session".into()));
    chat.api.set_identity(Some(Identity {
        session_id: "session".into(),
        device_id: uuid::Uuid::new_v4(),
        keys: Arc::new(DeviceKeys::generate().unwrap()),
    }));

    let _ = chat.log_out().await;

    let seen = transport.logged_out_at_send.lock().unwrap().clone();
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|logged_out| *logged_out), "{seen:?}");
}
