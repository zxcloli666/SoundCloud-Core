use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::contracts::DeviceStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterDevice {
    pub platform: String,
    pub name: String,
    pub app_version: String,
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceView {
    pub id: Uuid,
    pub platform: String,
    pub name: String,
    pub app_version: String,
    pub tier: String,
    pub status: DeviceStatus,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub history_unlocked: bool,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedKey {
    pub epoch: i32,
    #[serde(with = "super::b64")]
    pub sealed: Vec<u8>,
}
