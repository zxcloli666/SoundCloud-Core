use sc_chat::{Device, DeviceActions, DeviceId, DeviceStatus, Millis, Tier};

#[uniffi::remote(Enum)]
pub enum DeviceStatus {
    Active,
    Revoked,
}

#[uniffi::remote(Record)]
pub struct DeviceActions {
    pub confirm: bool,
    pub not_me: bool,
    pub remove: bool,
}

#[uniffi::remote(Record)]
pub struct Device {
    pub id: DeviceId,
    pub platform: String,
    pub name: String,
    pub app_version: String,
    pub tier: Tier,
    pub status: DeviceStatus,
    pub created_at: Millis,
    pub last_seen_at: Millis,
    pub history_unlocked: bool,
    pub current: bool,
    pub actions: DeviceActions,
}
