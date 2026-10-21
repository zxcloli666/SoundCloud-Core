use crate::api::Api;
use crate::api::path::query;
use crate::api::wire::{ConfigView, DeviceView, RegisterDevice, SealedKey};
use crate::contracts::{ChatResult, ConvId, DeviceId};

impl Api {
    pub async fn config(&self) -> ChatResult<ConfigView> {
        self.get("/chat/config", "").await
    }

    pub async fn register_device(&self, body: &RegisterDevice) -> ChatResult<DeviceView> {
        self.post("/chat/devices", body).await
    }

    pub async fn devices(&self) -> ChatResult<Vec<DeviceView>> {
        self.get("/chat/devices", "").await
    }

    pub async fn remove_device(&self, id: DeviceId) -> ChatResult<()> {
        self.delete(&format!("/chat/devices/{id}"), "").await
    }

    pub async fn remove_this_device(&self) -> ChatResult<()> {
        self.delete("/chat/devices/me", "").await
    }

    pub async fn confirm_device(&self, id: DeviceId) -> ChatResult<()> {
        self.post_empty(&format!("/chat/devices/{id}/confirm"))
            .await
    }

    pub async fn device_not_me(&self, id: DeviceId) -> ChatResult<()> {
        self.post_empty(&format!("/chat/devices/{id}/not-me")).await
    }

    pub async fn keys(
        &self,
        conversation_id: ConvId,
        from_epoch: Option<u32>,
    ) -> ChatResult<Vec<SealedKey>> {
        let query = query(&[("from_epoch", from_epoch.map(|epoch| epoch.to_string()))]);
        self.get(
            &format!("/chat/conversations/{conversation_id}/keys"),
            &query,
        )
        .await
    }
}
