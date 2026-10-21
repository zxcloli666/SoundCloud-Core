use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub seq: i64,
    pub client_id: Uuid,
    pub sender: String,
    pub sender_device: Option<Uuid>,
    pub class: String,
    pub epoch: i32,
    pub target_seq: Option<i64>,
    #[serde(default, with = "super::b64::option")]
    pub body: Option<Vec<u8>>,
    pub ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMessage {
    pub client_id: Uuid,
    pub class: String,
    pub epoch: i32,
    pub target_seq: Option<i64>,
    #[serde(with = "super::b64")]
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAnswer {
    pub seq: i64,
    pub ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagesPage {
    pub messages: Vec<MessageView>,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetTtl {
    pub ttl_s: Option<u32>,
}
