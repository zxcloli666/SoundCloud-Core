use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigView {
    pub enabled: bool,
    pub access: String,
    pub send_paused: bool,
    pub hold_max_ms: u64,
    pub server_now: i64,
    pub incarnation: Uuid,
    pub ock_current: u8,
    pub limits: LimitsView,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitsView {
    pub text_max: u32,
    pub embeds_max: u32,
    pub group_members_max: u32,
    pub devices_max: u32,
    pub envelope_max: u32,
    pub disappearing_ttls: Vec<u32>,
}
