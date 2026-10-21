use super::local::Local;
use super::{SyncEngine, devices, transient};
use crate::api::wire::DeviceView;
use crate::contracts::{ChatResult, CoreEvent, Device, DeviceAddedEvent, HistoryGate, Millis};

pub const UNLOCK_EAGER: usize = 20;
pub const LOCAL_MAX: usize = 100_000;

impl<L: Local> SyncEngine<L> {
    pub async fn devices(&self) -> ChatResult<Vec<Device>> {
        let views = self.api.devices().await?;
        self.devices_seen(&views).await?;
        Ok(devices::to_devices(&views, self.now()))
    }

    pub(super) async fn devices_seen(&self, views: &[DeviceView]) -> ChatResult<()> {
        let now = self.now();
        if let Some(me) = devices::current(views) {
            self.set_gate(devices::gate(me, now), me.created_at).await?;
        }
        let waiting: Vec<_> = devices::awaiting_alert(views, now)
            .into_iter()
            .map(|view| view.id)
            .collect();
        let fresh: Vec<Device> = devices::to_devices(views, now)
            .into_iter()
            .filter(|device| waiting.contains(&device.id))
            .filter(|device| self.with(|state| state.alerted.insert(device.id)))
            .collect();
        for device in fresh {
            self.events
                .emit(CoreEvent::DeviceAdded(DeviceAddedEvent { device }));
        }
        Ok(())
    }

    pub(super) async fn tick_gate(&self) -> ChatResult<()> {
        let now = self.now();
        let due = self.with(|state| {
            let gate = &state.status.history;
            gate.locked && gate.unlocks_at.is_some_and(|at| now >= at)
        });
        if !due {
            return Ok(());
        }
        let created = self
            .with(|state| state.device_created_at)
            .unwrap_or_default();
        self.set_gate(HistoryGate::default(), created).await
    }

    async fn set_gate(&self, gate: HistoryGate, created_at: Millis) -> ChatResult<()> {
        let opened = self.with(|state| {
            state.device_created_at = Some(created_at);
            state.status.history.locked && !gate.locked
        });
        self.update_status(|status| status.history = gate);
        if opened {
            self.unlock_history().await?;
        }
        Ok(())
    }

    async fn unlock_history(&self) -> ChatResult<()> {
        self.with(|state| state.locked_keys.clear());
        let local = self.store.known(LOCAL_MAX)?;
        let mut unlocked = Vec::new();
        for (id, _) in &local {
            if let Some(mut summary) = self.store.conversation(*id)?
                && summary.history_locked
            {
                summary.history_locked = false;
                unlocked.push(summary);
            }
        }
        if !unlocked.is_empty() {
            self.save(&unlocked)?;
            self.emit_summaries(unlocked);
        }
        for (id, _) in local.iter().take(UNLOCK_EAGER) {
            match self.refresh_keys(*id).await {
                Err(error) if transient(&error) => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }
}
