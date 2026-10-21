use std::sync::{Arc, Weak};
use std::time::Duration;

use sc_chat::Events;
use sc_chat::store::ExpiryTimer;
use sc_chat::store::expiry::NAP_MAX_MS;

use super::{Chat, Core};

const SOONEST_MS: u64 = 1_000;

impl Chat {
    pub(super) fn watch_expiry(self: &Arc<Self>, core: &Arc<Core>) {
        let events: Arc<dyn Events> = self.feed.clone();
        let timer = Arc::new(ExpiryTimer::new(
            core.store.clone(),
            self.clock.clone(),
            events,
        ));
        let chat = self.clone();
        let core = Arc::downgrade(core);
        self.handle.spawn(async move {
            while chat.still_open(&core) {
                let sweeper = timer.clone();
                let swept = chat.handle.spawn_blocking(move || sweeper.sweep()).await;
                let nap = swept.ok().and_then(Result::ok).unwrap_or(NAP_MAX_MS);
                tokio::time::sleep(Duration::from_millis(nap.clamp(SOONEST_MS, NAP_MAX_MS))).await;
            }
        });
    }

    fn still_open(&self, core: &Weak<Core>) -> bool {
        let Some(core) = core.upgrade() else {
            return false;
        };
        self.core().is_some_and(|open| Arc::ptr_eq(&open, &core))
    }
}
