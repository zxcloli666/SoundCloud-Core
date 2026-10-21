use sc_chat::{ChatResult, Device, DeviceId};

use crate::runtime::Chat;

impl Chat {
    pub async fn devices(&self) -> ChatResult<Vec<Device>> {
        self.ready()?.sync.devices().await
    }

    pub async fn confirm_device(&self, device_id: DeviceId) -> ChatResult<()> {
        self.api.confirm_device(device_id).await
    }

    pub async fn device_not_me(&self, device_id: DeviceId) -> ChatResult<()> {
        self.api.device_not_me(device_id).await
    }

    pub async fn remove_device(&self, device_id: DeviceId) -> ChatResult<()> {
        if self.status().device_id == Some(device_id) {
            return self.log_out().await;
        }
        self.api.remove_device(device_id).await
    }
}
