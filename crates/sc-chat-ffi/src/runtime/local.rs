use std::path::Path;
use std::sync::Arc;

use sc_chat::api::Api;
use sc_chat::crypto::DeviceKeys;
use sc_chat::outbox::Outbox;
use sc_chat::store::Store;
use sc_chat::sync::SyncEngine;
use sc_chat::{ChatError, ChatResult, DeviceId, Events, KeyStore};

pub const DATABASE: &str = "chat.db";

pub struct Core {
    pub device_id: DeviceId,
    pub keys: Arc<DeviceKeys>,
    pub store: Arc<Store>,
    pub outbox: Outbox,
    pub sync: SyncEngine,
}

impl Core {
    pub fn open(
        dir: &Path,
        keys: &dyn KeyStore,
        api: Arc<Api>,
        events: Arc<dyn Events>,
    ) -> ChatResult<Self> {
        std::fs::create_dir_all(dir).map_err(|_| ChatError::Store)?;
        let (device_id, device) = DeviceKeys::load_or_create(keys)?;
        let store = Arc::new(Store::open_keyed(&dir.join(DATABASE), keys)?);
        Ok(Self {
            device_id,
            keys: Arc::new(device),
            outbox: Outbox::new(store.clone()),
            sync: SyncEngine::new(api, store.clone(), events),
            store,
        })
    }
}
