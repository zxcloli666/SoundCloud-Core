use std::sync::Arc;

use sc_chat::api::Identity;
use sc_chat::api::wire::ConfigView;
use sc_chat::{
    Access, ChatConfig, ChatError, ChatLimits, ChatPhase, ChatResult, KeyStorage, UserId,
};

use super::{Chat, Core, Halt};

impl Chat {
    pub(super) async fn bring_up(self: &Arc<Self>, failures: &mut u32) -> Result<Arc<Core>, Halt> {
        let Some(session_id) = self.session.current() else {
            return Err(Halt::Phase(ChatPhase::SignedOut));
        };
        if self.session.logged_out() {
            return Err(Halt::Phase(ChatPhase::Unbound));
        }
        self.feed
            .update(|status| status.phase = ChatPhase::Starting);
        let core = self
            .local_core()
            .await
            .map_err(|error| self.fail(&error, failures))?;
        self.api.set_identity(Some(Identity {
            session_id,
            device_id: core.device_id,
            keys: core.keys.clone(),
        }));
        let view = self
            .api
            .config()
            .await
            .map_err(|error| self.fail(&error, failures))?;
        let user_id = view.user_id.clone();
        if !self.adopt_config(view).enabled {
            return Err(Halt::Phase(ChatPhase::Off));
        }
        let device = self
            .api
            .register_device(&self.device)
            .await
            .map_err(|error| self.fail(&error, failures))?;
        self.feed
            .update(|status| status.device_id = Some(device.id));
        if let Some(user_id) = user_id {
            self.adopt_user(&core, user_id, failures).await?;
        }
        self.feed.update(|status| {
            status.phase = ChatPhase::Ready;
            status.key_storage = KeyStorage::Os;
        });
        Ok(core)
    }

    pub async fn local_core(self: &Arc<Self>) -> ChatResult<Arc<Core>> {
        let _lifecycle = self.lifecycle.lock().await;
        if let Some(core) = self.core() {
            return Ok(core);
        }
        let chat = self.clone();
        let core = self
            .handle
            .spawn_blocking(move || {
                Core::open(
                    &chat.dir,
                    chat.keys.as_ref(),
                    chat.api.clone(),
                    chat.feed.clone(),
                )
            })
            .await
            .unwrap_or(Err(ChatError::Internal))?;
        let core = Arc::new(core);
        self.set_core(Some(core.clone()));
        self.watch_expiry(&core);
        Ok(core)
    }

    async fn adopt_user(
        &self,
        core: &Arc<Core>,
        user_id: UserId,
        failures: &mut u32,
    ) -> Result<(), Halt> {
        let store = core.store.clone();
        let me = user_id.clone();
        let other_account = self
            .handle
            .spawn_blocking(move || {
                let other = store.me()?.is_some_and(|known| known != me);
                if !other {
                    store.set_me(&me)?;
                }
                Ok(other)
            })
            .await
            .unwrap_or(Err(ChatError::Internal))
            .map_err(|error| self.fail(&error, failures))?;
        if other_account {
            let _ = self.wipe(false).await;
            return Err(Halt::Restart);
        }
        core.sync.set_user(Some(user_id.clone()));
        self.feed.update(|status| status.user_id = Some(user_id));
        Ok(())
    }

    fn adopt_config(&self, view: ConfigView) -> ChatConfig {
        let config = config_of(view);
        self.api.channel().use_ock(config.ock_current);
        self.api.channel().note_server_now(config.server_now);
        self.feed
            .update(|status| status.send_paused = config.send_paused);
        if let Ok(mut current) = self.config.write() {
            *current = Some(config.clone());
        }
        config
    }

    fn fail(&self, error: &ChatError, failures: &mut u32) -> Halt {
        *failures += 1;
        self.halt(error, *failures)
    }
}

fn config_of(view: ConfigView) -> ChatConfig {
    ChatConfig {
        enabled: view.enabled,
        access: if view.access == "all" {
            Access::All
        } else {
            Access::Beta
        },
        send_paused: view.send_paused,
        hold_max_ms: view.hold_max_ms,
        server_now: view.server_now,
        incarnation: view.incarnation,
        ock_current: view.ock_current,
        limits: ChatLimits {
            text_max: view.limits.text_max,
            embeds_max: view.limits.embeds_max,
            group_members_max: view.limits.group_members_max,
            devices_max: view.limits.devices_max,
            envelope_max: view.limits.envelope_max,
            disappearing_ttls: view.limits.disappearing_ttls,
        },
    }
}
