use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sc_chat::{CoreEvent, KeyName, Tier, TransportRequest, TransportResponse};
use sc_chat_ffi::{
    ChatClient, ChatKeys, ChatListener, ChatSetup, ChatTransport, KeyFailure, NetFailure,
};

#[derive(Default)]
pub struct MemoryKeys(Mutex<HashMap<KeyName, Vec<u8>>>);

impl MemoryKeys {
    pub fn names(&self) -> Vec<KeyName> {
        self.0.lock().unwrap().keys().copied().collect()
    }
}

impl ChatKeys for MemoryKeys {
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
pub struct Garbage(Mutex<Vec<TransportRequest>>);

impl Garbage {
    pub fn tiers_seen(&self) -> Vec<Tier> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.tier)
            .collect()
    }

    pub fn requests(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

#[async_trait]
impl ChatTransport for Garbage {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct, Tier::Relay]
    }

    async fn exchange(&self, request: TransportRequest) -> Result<TransportResponse, NetFailure> {
        self.0.lock().unwrap().push(request);
        Ok(TransportResponse {
            status: 200,
            body: vec![1, 2, 3],
            retry_after_ms: None,
        })
    }
}

#[derive(Default)]
pub struct Recorder(Mutex<Vec<CoreEvent>>);

impl Recorder {
    pub fn statuses(&self) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, CoreEvent::Status(_)))
            .count()
    }
}

impl ChatListener for Recorder {
    fn on_event(&self, event: CoreEvent) {
        self.0.lock().unwrap().push(event);
    }
}

pub struct Rig {
    pub client: Arc<ChatClient>,
    pub keys: Arc<MemoryKeys>,
    pub transport: Arc<Garbage>,
    pub events: Arc<Recorder>,
    pub dir: PathBuf,
}

impl Rig {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("sc-chat-ffi-{}", uuid::Uuid::new_v4()));
        let keys = Arc::new(MemoryKeys::default());
        let transport = Arc::new(Garbage::default());
        let events = Arc::new(Recorder::default());
        let setup = ChatSetup {
            data_dir: dir.to_string_lossy().into_owned(),
            platform: "android".into(),
            device_name: "Test phone".into(),
            app_version: "0.0.0".into(),
        };
        let client =
            ChatClient::new(setup, keys.clone(), transport.clone(), events.clone()).unwrap();
        Self {
            client,
            keys,
            transport,
            events,
            dir,
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub async fn until(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(10), "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub async fn settle<F: Future>(work: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .expect("the call answers")
}
