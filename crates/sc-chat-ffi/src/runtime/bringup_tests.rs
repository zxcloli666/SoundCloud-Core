use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use sc_chat::{ChatPhase, CoreEvent, KeyName, Tier, TransportRequest, TransportResponse};
use tokio::runtime::Handle;

use super::{Chat, Halt};
use crate::platform::{ChatKeys, ChatListener, ChatTransport, KeyFailure, NetFailure};
use crate::setup::ChatSetup;

struct Refused;

impl ChatKeys for Refused {
    fn load(&self, _name: KeyName) -> Result<Option<Vec<u8>>, KeyFailure> {
        Err(denied())
    }

    fn save(&self, _name: KeyName, _secret: Vec<u8>) -> Result<(), KeyFailure> {
        Err(denied())
    }

    fn delete(&self, _name: KeyName) -> Result<(), KeyFailure> {
        Err(denied())
    }
}

fn denied() -> KeyFailure {
    KeyFailure::Denied {
        reason: "the user pressed Deny".into(),
    }
}

struct Nowhere;

#[async_trait]
impl ChatTransport for Nowhere {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct]
    }

    async fn exchange(&self, _request: TransportRequest) -> Result<TransportResponse, NetFailure> {
        Err(NetFailure::Unreachable)
    }
}

struct Silent;

impl ChatListener for Silent {
    fn on_event(&self, _event: CoreEvent) {}
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn chat() -> (Arc<Chat>, Scratch) {
    let dir = std::env::temp_dir().join(format!("sc-chat-bringup-{}", uuid::Uuid::new_v4()));
    let setup = ChatSetup {
        data_dir: dir.to_string_lossy().into_owned(),
        platform: "android".into(),
        device_name: "Test phone".into(),
        app_version: "0.0.0".into(),
    };
    let chat = Arc::new(Chat::new(
        Handle::current(),
        setup,
        Arc::new(Refused),
        Arc::new(Nowhere),
        Arc::new(Silent),
    ));
    (chat, Scratch(dir))
}

#[tokio::test]
async fn a_restart_the_attempt_already_saw_does_not_cut_it_short() {
    let (chat, _scratch) = chat();
    chat.session.replace(Some("session".into()));
    let seen = chat.session.seen();
    chat.restart();

    let halt = chat.bring_up(&seen, &mut 0).await.err();

    assert_eq!(halt, Some(Halt::Phase(ChatPhase::KeychainDenied)));
}

#[tokio::test]
async fn a_sign_out_after_the_attempt_started_cuts_it_short() {
    let (chat, _scratch) = chat();
    chat.session.replace(Some("session".into()));
    let seen = chat.session.seen();
    chat.session.replace(None);

    let halt = chat.bring_up(&seen, &mut 0).await.err();

    assert_eq!(halt, Some(Halt::Restart));
}
