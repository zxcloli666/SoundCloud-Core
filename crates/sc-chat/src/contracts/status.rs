use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::ids::{DeviceId, Millis, UserId};

pub const DIRECT_MAX_BYTES: u32 = 256 * 1024;
pub const PRO_MAX_BYTES: u32 = 24 * 1024;
pub const VISIBLE_HOLD_MAX_MS: u64 = 20_000;
pub const PRO_HOLD_MAX_MS: u64 = 15_000;
pub const REDUCED_HOLD_MS: u64 = 8_000;
pub const HIDDEN_SYNC_EVERY_MS: u64 = 60_000;
pub const DISAPPEARING_TTLS: [u32; 3] = [3_600, 86_400, 604_800];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Direct,
    Relay,
    Pro,
}

impl Tier {
    pub fn wire(self) -> sc_chat_wire::proto::Tier {
        match self {
            Tier::Direct => sc_chat_wire::proto::Tier::Direct,
            Tier::Relay => sc_chat_wire::proto::Tier::Relay,
            Tier::Pro => sc_chat_wire::proto::Tier::Pro,
        }
    }

    pub fn max_bytes(self) -> u32 {
        match self {
            Tier::Pro => PRO_MAX_BYTES,
            Tier::Direct | Tier::Relay => DIRECT_MAX_BYTES,
        }
    }

    pub fn hold_max_ms(self) -> u64 {
        match self {
            Tier::Pro => PRO_HOLD_MAX_MS,
            Tier::Direct | Tier::Relay => VISIBLE_HOLD_MAX_MS,
        }
    }
}

pub fn is_disappearing_ttl(ttl_s: u32) -> bool {
    DISAPPEARING_TTLS.contains(&ttl_s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatPhase {
    Off,
    SignedOut,
    Unbound,
    BoundOther,
    Starting,
    Ready,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStorage {
    Os,
    File,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryGate {
    pub locked: bool,
    pub unlocks_at: Option<Millis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

impl ChatStatus {
    pub fn off() -> Self {
        Self {
            phase: ChatPhase::Off,
            user_id: None,
            device_id: None,
            tier: None,
            online: false,
            visible: false,
            send_paused: false,
            history: HistoryGate::default(),
            key_storage: KeyStorage::Os,
            unread: 0,
            mentions: 0,
            requests: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Beta,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatLimits {
    pub text_max: u32,
    pub embeds_max: u32,
    pub group_members_max: u32,
    pub devices_max: u32,
    pub envelope_max: u32,
    pub disappearing_ttls: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatConfig {
    pub enabled: bool,
    pub access: Access,
    pub send_paused: bool,
    pub hold_max_ms: u64,
    pub server_now: Millis,
    pub incarnation: Uuid,
    pub ock_current: u8,
    pub limits: ChatLimits,
}
