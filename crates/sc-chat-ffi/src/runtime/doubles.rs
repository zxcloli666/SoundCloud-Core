use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sc_chat::api::{Api, CallOptions, Exchange, Signer};
use sc_chat::channel::{Channel, InnerAnswer, InnerCall};
use sc_chat::{
    ChatError, ChatResult, Clock, CoreEvent, KeyName, Tier, TransportRequest, TransportResponse,
};
use tokio::runtime::Handle;

use super::Chat;
use super::clock::SystemClock;
use crate::platform::{ChatKeys, ChatListener, ChatTransport, KeyFailure, NetFailure, Wire};
use crate::setup::ChatSetup;

pub struct Refused;

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

pub struct Nowhere;

#[async_trait]
impl ChatTransport for Nowhere {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct]
    }

    async fn exchange(&self, _request: TransportRequest) -> Result<TransportResponse, NetFailure> {
        Err(NetFailure::Unreachable)
    }
}

pub struct Silent;

impl ChatListener for Silent {
    fn on_event(&self, _event: CoreEvent) {}
}

pub struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn chat() -> (Arc<Chat>, Scratch) {
    chat_on(Arc::new(Nowhere))
}

pub fn chat_on(transport: Arc<dyn ChatTransport>) -> (Arc<Chat>, Scratch) {
    let (setup, scratch) = setup();
    let chat = Chat::new(
        Handle::current(),
        setup,
        Arc::new(Refused),
        transport,
        Arc::new(Silent),
    );
    (Arc::new(chat), scratch)
}

pub fn chat_at(origin: Arc<Origin>, listener: Arc<dyn ChatListener>) -> (Arc<Chat>, Scratch) {
    let (setup, scratch) = setup();
    let channel = Arc::new(Channel::new(Arc::new(SystemClock)));
    let api = Api::with_exchange(channel, Arc::new(Wire(Arc::new(Nowhere))), origin);
    let chat = Chat::with_api(
        Handle::current(),
        setup,
        Arc::new(Vault::default()),
        api,
        listener,
    );
    (Arc::new(chat), scratch)
}

fn setup() -> (ChatSetup, Scratch) {
    let dir = std::env::temp_dir().join(format!("sc-chat-bringup-{}", uuid::Uuid::new_v4()));
    let setup = ChatSetup {
        data_dir: dir.to_string_lossy().into_owned(),
        platform: "android".into(),
        device_name: "Test phone".into(),
        app_version: "0.0.0".into(),
    };
    (setup, Scratch(dir))
}

#[derive(Default)]
pub struct Vault(Mutex<HashMap<KeyName, Vec<u8>>>);

impl ChatKeys for Vault {
    fn load(&self, name: KeyName) -> Result<Option<Vec<u8>>, KeyFailure> {
        Ok(self.0.lock().unwrap().get(&name).cloned())
    }

    fn save(&self, name: KeyName, secret: Vec<u8>) -> Result<(), KeyFailure> {
        self.0.lock().unwrap().insert(name, secret);
        Ok(())
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyFailure> {
        self.0.lock().unwrap().remove(&name);
        Ok(())
    }
}

#[derive(Default)]
pub struct Origin(Mutex<Vec<String>>);

impl Origin {
    pub fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait]
impl Exchange for Origin {
    async fn exchange(
        &self,
        call: &InnerCall,
        _signer: &Signer,
        _tier: Tier,
        _options: &CallOptions,
    ) -> ChatResult<InnerAnswer> {
        self.0
            .lock()
            .unwrap()
            .push(format!("{} {}", call.method, call.path));
        if call.path != "/chat/config" {
            return Err(ChatError::Offline);
        }
        Ok(InnerAnswer {
            status: 200,
            headers: Vec::new(),
            body: paused_config().into_bytes(),
        })
    }
}

fn paused_config() -> String {
    let now = SystemClock.now_ms();
    format!(
        r#"{{"enabled":true,"access":"all","send_paused":true,"hold_max_ms":20000,"server_now":{now},"incarnation":"00000000-0000-0000-0000-000000000000","ock_current":1,"limits":{{"text_max":4096,"embeds_max":3,"group_members_max":200,"devices_max":10,"envelope_max":65536,"disappearing_ttls":[3600]}}}}"#
    )
}
