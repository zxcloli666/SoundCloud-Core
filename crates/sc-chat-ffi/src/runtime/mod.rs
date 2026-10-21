mod bringup;
mod clock;
mod expiry;
mod feed;
mod lifecycle;
mod local;
mod retry;
mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Instant;

use sc_chat::api::Api;
use sc_chat::api::wire::RegisterDevice;
use sc_chat::channel::Channel;
use sc_chat::crypto::DeviceKeys;
use sc_chat::{ChatConfig, ChatError, ChatResult, ChatStatus, Clock, ConvId, DeviceId};
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::platform::{ChatKeys, ChatListener, ChatTransport, Keys, Wire};
use crate::setup::ChatSetup;
use clock::SystemClock;
use feed::Feed;
use session::Session;

pub use lifecycle::Halt;
pub use local::Core;

pub struct Held {
    pub device_id: DeviceId,
    pub keys: Arc<DeviceKeys>,
    pub since: Instant,
}

pub struct Chat {
    pub handle: Handle,
    pub api: Arc<Api>,
    pub keys: Arc<Keys>,
    dir: PathBuf,
    device: RegisterDevice,
    clock: Arc<dyn Clock>,
    feed: Arc<Feed>,
    session: Session,
    core: RwLock<Option<Arc<Core>>>,
    config: RwLock<Option<ChatConfig>>,
    visible: AtomicBool,
    restarts: watch::Sender<u64>,
    lifecycle: tokio::sync::Mutex<()>,
    held: Mutex<HashMap<String, Held>>,
}

impl Chat {
    pub fn new(
        handle: Handle,
        setup: ChatSetup,
        keys: Arc<dyn ChatKeys>,
        transport: Arc<dyn ChatTransport>,
        listener: Arc<dyn ChatListener>,
    ) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let channel = Arc::new(Channel::new(clock.clone()));
        Self {
            handle,
            api: Arc::new(Api::new(channel, Arc::new(Wire(transport)))),
            keys: Arc::new(Keys(keys)),
            dir: PathBuf::from(setup.data_dir),
            device: RegisterDevice {
                platform: setup.platform,
                name: setup.device_name,
                app_version: setup.app_version,
                caps: Vec::new(),
            },
            clock,
            feed: Arc::new(Feed::new(listener)),
            session: Session::default(),
            core: RwLock::new(None),
            config: RwLock::new(None),
            visible: AtomicBool::new(false),
            restarts: watch::Sender::new(0),
            lifecycle: tokio::sync::Mutex::new(()),
            held: Mutex::new(HashMap::new()),
        }
    }

    pub fn status(&self) -> ChatStatus {
        self.feed.status()
    }

    pub fn core(&self) -> Option<Arc<Core>> {
        self.core.read().ok().and_then(|core| core.clone())
    }

    pub fn ready(&self) -> ChatResult<Arc<Core>> {
        self.core().ok_or(ChatError::NotReady)
    }

    pub fn config(&self) -> Option<ChatConfig> {
        self.config.read().ok().and_then(|config| config.clone())
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    pub fn session_id(&self) -> ChatResult<String> {
        self.session.current().ok_or(ChatError::NotReady)
    }

    pub fn held(&self) -> MutexGuard<'_, HashMap<String, Held>> {
        lock(&self.held)
    }

    pub fn set_visible(&self, visible: bool) {
        if self.visible.swap(visible, Ordering::Relaxed) == visible {
            return;
        }
        self.feed.update(|status| status.visible = visible);
        if let Some(core) = self.core() {
            core.sync.set_visible(visible);
        }
    }

    pub fn set_focus(&self, conversation_id: Option<ConvId>) {
        if let Some(core) = self.core() {
            core.sync.set_focus(conversation_id);
        }
    }

    pub fn kick(&self) {
        if let Some(core) = self.core() {
            core.sync.kick();
        }
    }

    pub fn network_changed(&self) {
        if let Some(core) = self.core() {
            core.sync.reset_network();
        }
    }

    pub async fn blocking<T, F>(&self, work: F) -> ChatResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Core) -> ChatResult<T> + Send + 'static,
    {
        let core = self.ready()?;
        self.handle
            .spawn_blocking(move || work(&core))
            .await
            .unwrap_or(Err(ChatError::Internal))
    }

    fn restart(&self) {
        self.restarts.send_modify(|generation| *generation += 1);
    }

    fn generation(&self) -> u64 {
        *self.restarts.borrow()
    }

    async fn restarted(&self, generation: u64) {
        let mut restarts = self.restarts.subscribe();
        let _ = restarts.wait_for(|now| *now != generation).await;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
