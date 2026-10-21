mod apply;
pub mod cipher;
mod conversations;
pub mod devices;
mod gate;
mod history;
mod incoming;
mod jitter;
mod keys;
pub mod local;
mod marks;
mod outbox;
mod reconcile;
mod rooms;
mod rows;
mod run;
mod signal;
mod state;
mod status;
pub mod summary;
mod system;
mod tally;
mod tiers;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub use cipher::{Cipher, DeviceCipher};
pub use local::{Local, LocalStore};
pub use marks::MarkBatch;
pub use tiers::{BACKOFF_MAX_MS, MID_HOLD_MIN_MS};

pub const SHORT_POLL_GAP_MS: u64 = 3_000;

use crate::api::Api;
use crate::contracts::{
    ChatError, ChatResult, ConvId, Events, HIDDEN_SYNC_EVERY_MS, Millis, Seq, Tier, UserId,
};
use crate::store::Store;
use signal::Signal;
use state::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollPlan {
    pub tier: Tier,
    pub hold_ms: u64,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    Changed,
    Idle,
    Reset,
    Retry { after_ms: u64 },
}

pub struct SyncEngine<L: Local = LocalStore> {
    api: Arc<Api>,
    store: Arc<L>,
    events: Arc<dyn Events>,
    cipher: Arc<dyn Cipher>,
    visible: AtomicBool,
    stopped: AtomicBool,
    wake: Signal,
    halt: Signal,
    state: Mutex<State>,
}

impl SyncEngine<LocalStore> {
    pub fn new(api: Arc<Api>, store: Arc<Store>, events: Arc<dyn Events>) -> Self {
        let cipher = Arc::new(DeviceCipher::new(api.clone()));
        Self::with_parts(api, Arc::new(LocalStore::new(store)), events, cipher)
    }

    pub fn store(&self) -> &Arc<Store> {
        self.store.store()
    }
}

impl<L: Local> SyncEngine<L> {
    pub fn with_parts(
        api: Arc<Api>,
        store: Arc<L>,
        events: Arc<dyn Events>,
        cipher: Arc<dyn Cipher>,
    ) -> Self {
        Self {
            api,
            store,
            events,
            cipher,
            visible: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            wake: Signal::default(),
            halt: Signal::default(),
            state: Mutex::new(State::default()),
        }
    }

    pub fn api(&self) -> &Arc<Api> {
        &self.api
    }

    pub fn local(&self) -> &Arc<L> {
        &self.store
    }

    pub fn events(&self) -> &Arc<dyn Events> {
        &self.events
    }

    pub fn visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }

    pub fn set_visible(&self, visible: bool) -> bool {
        let was = self.visible.swap(visible, Ordering::Relaxed);
        self.update_status(|status| status.visible = visible);
        let shown = visible && !was;
        if shown {
            self.wake.raise();
        }
        shown
    }

    pub fn focus(&self) -> Option<ConvId> {
        self.with(|state| state.focus)
    }

    pub fn set_focus(&self, conversation_id: Option<ConvId>) {
        let changed = self.with(|state| {
            let changed = state.focus != conversation_id;
            state.focus = conversation_id;
            changed
        });
        if changed {
            self.wake.raise();
        }
    }

    pub fn set_user(&self, user_id: Option<UserId>) {
        self.with(|state| state.user = user_id);
    }

    pub fn set_hold_cap(&self, hold_max_ms: u64) {
        self.with(|state| state.hold_cap = hold_max_ms);
    }

    pub fn kick(&self) {
        self.wake.raise();
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.wake.raise();
        self.halt.raise();
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    pub fn reset_network(&self) {
        self.with(|state| state.tiers.reset());
        self.wake.raise();
    }

    pub fn plan(&self, tier: Tier) -> PollPlan {
        let visible = self.visible();
        self.with(|state| match (state.more, visible) {
            (true, _) => PollPlan {
                tier,
                hold_ms: 0,
                delay_ms: 0,
            },
            (false, true) => {
                let hold_ms = state.tiers.hold(tier).min(state.hold_cap);
                PollPlan {
                    tier,
                    hold_ms,
                    delay_ms: if hold_ms == 0 { SHORT_POLL_GAP_MS } else { 0 },
                }
            }
            (false, false) => PollPlan {
                tier,
                hold_ms: 0,
                delay_ms: HIDDEN_SYNC_EVERY_MS,
            },
        })
    }

    pub fn next_plan(&self) -> ChatResult<PollPlan> {
        let order = self.api.transport().tiers();
        let now = self.now();
        let tier = self
            .with(|state| state.tiers.pick(&order, now))
            .ok_or(ChatError::Offline)?;
        Ok(self.plan(tier))
    }

    pub fn queue_read(&self, conversation_id: ConvId, seq: Seq) {
        if self.with(|state| state.marks.read(conversation_id, seq)) {
            self.read_locally(conversation_id, seq);
            self.wake.raise();
        }
    }

    pub(crate) fn now(&self) -> Millis {
        self.api.channel().now_ms()
    }

    pub(crate) fn with<R>(&self, change: impl FnOnce(&mut State) -> R) -> R {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        change(&mut state)
    }
}

pub(crate) fn fatal(error: &ChatError) -> bool {
    matches!(
        error,
        ChatError::DeviceRevoked
            | ChatError::ReloginRequired
            | ChatError::SessionBoundOther
            | ChatError::Disabled
            | ChatError::NotReady
    )
}

pub(crate) fn transient(error: &ChatError) -> bool {
    fatal(error)
        || matches!(
            error,
            ChatError::Offline | ChatError::Busy { .. } | ChatError::Internal | ChatError::Store
        )
}
