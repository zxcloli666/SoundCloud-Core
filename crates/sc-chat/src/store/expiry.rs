use std::sync::Arc;

use super::Store;
use crate::contracts::{ChatResult, Clock, CoreEvent, Events, GoneEvent};

pub const NAP_MAX_MS: u64 = 15_000;

pub struct ExpiryTimer {
    store: Arc<Store>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn Events>,
}

impl ExpiryTimer {
    pub fn new(store: Arc<Store>, clock: Arc<dyn Clock>, events: Arc<dyn Events>) -> Self {
        Self {
            store,
            clock,
            events,
        }
    }

    pub fn sweep(&self) -> ChatResult<u64> {
        let now = self.clock.now_ms();
        let gone = self.store.purge_expired(now)?;
        if !gone.is_empty() {
            self.events.emit(CoreEvent::Gone(GoneEvent {
                conversations: Vec::new(),
                messages: gone,
            }));
        }
        let nap = match self.store.next_expiry()? {
            Some(at) => (at - now).clamp(0, NAP_MAX_MS as i64) as u64,
            None => NAP_MAX_MS,
        };
        Ok(nap)
    }

    pub async fn run(&self) {
        loop {
            let nap = self.sweep().unwrap_or(NAP_MAX_MS);
            self.clock.sleep(nap).await;
        }
    }
}
