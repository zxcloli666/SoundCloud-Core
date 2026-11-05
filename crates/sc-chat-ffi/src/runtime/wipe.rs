use sc_chat::api::{Api, Identity};
use sc_chat::crypto::DeviceKeys;
use sc_chat::store::{remove_database, remove_orphans};
use sc_chat::{ChatError, ChatResult, HistoryGate, KeyName, KeyStore};

use super::Chat;
use super::local::DATABASE;

impl Chat {
    pub async fn log_out(&self) -> ChatResult<()> {
        self.session.log_out();
        let _ = self.api.remove_this_device().await;
        self.wipe(true).await
    }

    pub(super) async fn drop_device(&self, identity: Option<Identity>) {
        let _ = self.wipe(true).await;
        let Some(identity) = identity else {
            return;
        };
        let api = Api::new(self.api.channel().clone(), self.api.transport().clone());
        api.set_identity(Some(identity));
        let _ = api.remove_this_device().await;
    }

    pub async fn wipe(&self, forget_device: bool) -> ChatResult<()> {
        let _lifecycle = self.lifecycle.lock().await;
        self.forget_user();
        let core = self.core.write().ok().and_then(|mut core| core.take());
        self.api.set_identity(None);
        self.session.wiped();
        self.restart();
        let keys = self.keys.clone();
        let database = self.dir.join(DATABASE);
        let wiped = self
            .handle
            .spawn_blocking(move || {
                let store = match core {
                    Some(core) => core.store.wipe(),
                    None if forget_device => remove_database(&database),
                    None => Ok(()),
                };
                let orphans = remove_orphans(&database);
                let secrets = if forget_device {
                    let device = DeviceKeys::erase(keys.as_ref());
                    let database_key = keys.delete(KeyName::Database);
                    device.and(database_key)
                } else {
                    Ok(())
                };
                store.and(orphans).and(secrets)
            })
            .await
            .unwrap_or(Err(ChatError::Internal));
        self.feed.update(|status| {
            status.device_id = None;
            status.history = HistoryGate::default();
            status.unread = 0;
            status.mentions = 0;
            status.requests = 0;
        });
        wiped
    }
}

#[cfg(test)]
#[path = "wipe_tests.rs"]
mod tests;
