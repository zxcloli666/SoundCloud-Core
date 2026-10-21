use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use sc_chat::api::{Api, Identity};
use sc_chat::crypto::DeviceKeys;
use sc_chat::{ChatError, ChatPhase, ChatResult, HistoryGate, KeyName, KeyStore};

use super::session::{AuthMove, auth_move};
use super::{Chat, Core, retry};

const OFF_RECHECK_MS: u64 = 3_600_000;
const OFF_SPREAD_MS: u64 = 600_000;
const STUB_RETRY: Duration = Duration::from_secs(60);
const FIRST_FAILURE: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halt {
    Restart,
    Phase(ChatPhase),
    Retry(Duration),
}

impl Chat {
    pub async fn run(self: Arc<Self>) {
        let mut failures = 0;
        loop {
            let generation = self.generation();
            let halt = match self.bring_up(&mut failures).await {
                Ok(core) => {
                    failures = 0;
                    self.drive(core, generation).await
                }
                Err(halt) => halt,
            };
            self.settle(halt, generation).await;
        }
    }

    pub fn set_session(self: &Arc<Self>, session_id: Option<String>) {
        let signed_in = session_id.is_some();
        let previous = self.session.replace(session_id.clone());
        let changed = previous != session_id;
        match auth_move(previous.is_some(), signed_in, changed, self.status().phase) {
            AuthMove::Restart => {
                self.session.log_in();
                self.restart();
            }
            AuthMove::DropDevice => {
                let chat = self.clone();
                let identity = self.api.identity();
                self.handle
                    .spawn(async move { chat.drop_device(identity).await });
            }
            AuthMove::Stay => {}
        }
    }

    pub async fn log_out(&self) -> ChatResult<()> {
        let _ = self.api.remove_this_device().await;
        self.session.log_out();
        self.wipe(true).await
    }

    async fn drop_device(&self, identity: Option<Identity>) {
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
        self.restart();
        let keys = self.keys.clone();
        let wiped = self
            .handle
            .spawn_blocking(move || {
                if let Some(core) = core {
                    core.store.wipe()?;
                }
                if forget_device {
                    DeviceKeys::erase(keys.as_ref())?;
                    keys.delete(KeyName::Database)?;
                }
                Ok(())
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

    pub fn halt(&self, error: &ChatError, failures: u32) -> Halt {
        match error {
            ChatError::ReloginRequired => Halt::Phase(ChatPhase::Unbound),
            ChatError::SessionBoundOther => Halt::Phase(ChatPhase::BoundOther),
            ChatError::DeviceRevoked => Halt::Phase(ChatPhase::Revoked),
            ChatError::Disabled => Halt::Phase(ChatPhase::Off),
            ChatError::Unimplemented => Halt::Retry(STUB_RETRY),
            ChatError::Busy { retry_ms } => Halt::Retry(retry::busy(*retry_ms)),
            ChatError::Clock { server_now } => {
                self.api.channel().note_server_now(*server_now);
                Halt::Retry(retry::clock(failures))
            }
            _ => Halt::Retry(retry::backoff(failures)),
        }
    }

    pub(super) fn set_core(&self, core: Option<Arc<Core>>) {
        if let Ok(mut current) = self.core.write() {
            *current = core;
        }
    }

    fn forget_user(&self) {
        if let Some(core) = self.core() {
            core.sync.set_user(None);
        }
        self.feed.update(|status| status.user_id = None);
    }

    async fn drive(&self, core: Arc<Core>, generation: u64) -> Halt {
        core.sync.set_visible(self.visible.load(Ordering::Relaxed));
        if let Some(config) = self.config() {
            core.sync.set_hold_cap(config.hold_max_ms);
        }
        tokio::select! {
            ran = core.sync.run(self.clock.as_ref()) => match ran {
                Ok(()) => Halt::Restart,
                Err(error) => self.halt(&error, FIRST_FAILURE),
            },
            () = self.restarted(generation) => Halt::Restart,
        }
    }

    async fn settle(&self, halt: Halt, generation: u64) {
        match halt {
            Halt::Restart => {}
            Halt::Retry(pause) => {
                self.feed.update(|status| status.online = false);
                self.pause(pause, generation).await;
            }
            Halt::Phase(phase) => {
                if signed_out(phase) {
                    self.forget_user();
                }
                self.feed.update(|status| {
                    status.phase = phase;
                    status.online = false;
                });
                let generation = if phase == ChatPhase::Revoked {
                    let _ = self.wipe(true).await;
                    generation + 1
                } else {
                    generation
                };
                if phase == ChatPhase::Off {
                    let recheck = OFF_RECHECK_MS + retry::jitter(OFF_SPREAD_MS);
                    self.pause(Duration::from_millis(recheck), generation).await;
                } else {
                    self.restarted(generation).await;
                }
            }
        }
    }

    async fn pause(&self, pause: Duration, generation: u64) {
        tokio::select! {
            () = tokio::time::sleep(pause) => {}
            () = self.restarted(generation) => {}
        }
    }
}

fn signed_out(phase: ChatPhase) -> bool {
    matches!(
        phase,
        ChatPhase::SignedOut | ChatPhase::Unbound | ChatPhase::BoundOther | ChatPhase::Revoked
    )
}
