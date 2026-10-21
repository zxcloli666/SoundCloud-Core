use serde::{Deserialize, Serialize};

use super::conversation::ConversationSummary;
use super::device::Device;
use super::ids::{ConvId, UserId};
use super::message::{GoneMessage, Message, PeerMark};
use super::status::ChatStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagesOrigin {
    Sync,
    History,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagesEvent {
    pub conversation_id: ConvId,
    pub origin: MessagesOrigin,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMarks {
    pub conversation_id: ConvId,
    pub marks: Vec<PeerMark>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub reset: bool,
    pub conversations: Vec<ConversationSummary>,
    pub marks: Vec<ConversationMarks>,
    pub requests: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingEvent {
    pub conversation_id: ConvId,
    pub users: Vec<UserId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoneEvent {
    pub conversations: Vec<ConvId>,
    pub messages: Vec<GoneMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAddedEvent {
    pub device: Device,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    Status(ChatStatus),
    Sync(SyncEvent),
    Messages(MessagesEvent),
    Typing(TypingEvent),
    Gone(GoneEvent),
    DeviceAdded(DeviceAddedEvent),
}
