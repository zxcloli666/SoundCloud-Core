use serde::{Deserialize, Serialize};

use super::ids::{DeviceId, Millis};
use super::status::Tier;

pub const DEVICE_AGE_RULE_MS: Millis = 72 * 3_600_000;
pub const DEVICES_MAX: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Active,
    Revoked,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceActions {
    pub confirm: bool,
    pub not_me: bool,
    pub remove: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
