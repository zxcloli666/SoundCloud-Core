use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::device::DeviceView;
use super::message::MessageView;

pub const KNOWN_MAX: usize = 64;
pub const MARKS_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequest {
    pub pos: u64,
    pub inc: Option<Uuid>,
    pub hold_ms: u64,
    pub max_bytes: u32,
    pub known: Vec<(Uuid, i64)>,
    pub focus: Option<Uuid>,
    pub read: Vec<(Uuid, i64)>,
    pub delivered: Vec<(Uuid, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncAnswer {
    pub pos: u64,
    pub inc: Uuid,
    pub reset: bool,
    pub convs: Vec<SyncConversation>,
    pub gone: Vec<Uuid>,
    #[serde(default)]
    pub gone_msgs: Vec<(Uuid, i64)>,
    pub devices: Vec<DeviceView>,
    pub requests_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConversation {
    pub id: Uuid,
    pub last_seq: i64,
    pub key_epoch: i32,
    pub messages: Vec<MessageView>,
    pub more: bool,
    pub marks: Vec<PeerMarkView>,
    pub typing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerMarkView {
    pub user_id: String,
    pub read_seq: i64,
    pub delivered_seq: i64,
}
