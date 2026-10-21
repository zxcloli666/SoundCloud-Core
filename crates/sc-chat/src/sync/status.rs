use super::SyncEngine;
use super::local::Local;
use crate::contracts::{ChatStatus, CoreEvent, Tier};

impl<L: Local> SyncEngine<L> {
    pub fn status(&self) -> ChatStatus {
        self.with(|state| state.status.clone())
    }

    pub fn update_status(&self, change: impl FnOnce(&mut ChatStatus)) {
        let changed = self.with(|state| {
            let before = state.status.clone();
            change(&mut state.status);
            (state.status != before).then(|| state.status.clone())
        });
        if let Some(status) = changed {
            self.events.emit(CoreEvent::Status(status));
        }
    }

    pub(super) fn reached(&self, tier: Tier) {
        self.update_status(|status| {
            status.online = true;
            status.tier = Some(tier);
        });
    }

    pub(super) fn lost(&self) {
        self.update_status(|status| status.online = false);
    }

    pub(super) fn note_requests(&self, requests: u32) -> bool {
        let before = self.with(|state| state.status.requests);
        self.update_status(|status| status.requests = requests);
        before != requests
    }
}
