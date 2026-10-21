use sc_chat::{ChatPhase, ChatStatus, DeviceId, HistoryGate, KeyStorage, Millis, Tier, UserId};

#[uniffi::remote(Enum)]
pub enum Tier {
    Direct,
    Relay,
    Pro,
}

#[uniffi::remote(Enum)]
pub enum ChatPhase {
    Off,
    SignedOut,
    Unbound,
    BoundOther,
    Starting,
    Ready,
    Revoked,
}

#[uniffi::remote(Enum)]
pub enum KeyStorage {
    Os,
    File,
}

#[uniffi::remote(Record)]
pub struct HistoryGate {
    pub locked: bool,
    pub unlocks_at: Option<Millis>,
}

#[uniffi::remote(Record)]
pub struct ChatStatus {
    pub phase: ChatPhase,
    pub user_id: Option<UserId>,
    pub device_id: Option<DeviceId>,
    pub tier: Option<Tier>,
    pub online: bool,
    pub visible: bool,
    pub send_paused: bool,
    pub history: HistoryGate,
    pub key_storage: KeyStorage,
    pub unread: u32,
    pub mentions: u32,
    pub requests: u32,
}
