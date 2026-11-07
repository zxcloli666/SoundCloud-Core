use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::ConversationView;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicView {
    pub conversation: ConversationView,
    pub online: u32,
    #[serde(default)]
    pub mute: Option<MuteView>,
    #[serde(default)]
    pub newcomer: Option<NewcomerView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuteView {
    pub until: i64,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewcomerView {
    #[serde(default)]
    pub until: Option<i64>,
    pub body_max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicMute {
    pub seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Muted {
    pub until: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlowMode {
    pub seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: Uuid,
    pub by: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub detail: Value,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogPage {
    pub entries: Vec<LogEntry>,
    pub next: Option<Uuid>,
}
