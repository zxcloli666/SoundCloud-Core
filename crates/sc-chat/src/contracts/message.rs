use serde::{Deserialize, Serialize};

use super::conversation::MemberRole;
use super::embed::Embed;
use super::ids::{ClientId, ConvId, DeviceId, Millis, Seq, UserId};

pub const TEXT_MAX: usize = sc_chat_wire::msg::TEXT_MAX;
pub const EMBEDS_MAX: usize = sc_chat_wire::msg::EMBEDS_MAX;
pub const EDIT_WINDOW_MS: Millis = 48 * 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageClass {
    Post,
    Edit,
    React,
    Pin,
    Meta,
    System,
}

impl MessageClass {
    pub fn wire(self) -> sc_chat_wire::msg::Class {
        match self {
            MessageClass::Post => sc_chat_wire::msg::Class::Post,
            MessageClass::Edit => sc_chat_wire::msg::Class::Edit,
            MessageClass::React => sc_chat_wire::msg::Class::React,
            MessageClass::Pin => sc_chat_wire::msg::Class::Pin,
            MessageClass::Meta => sc_chat_wire::msg::Class::Meta,
            MessageClass::System => sc_chat_wire::msg::Class::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        self.wire().as_str()
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match sc_chat_wire::msg::Class::parse(raw)? {
            sc_chat_wire::msg::Class::Post => MessageClass::Post,
            sc_chat_wire::msg::Class::Edit => MessageClass::Edit,
            sc_chat_wire::msg::Class::React => MessageClass::React,
            sc_chat_wire::msg::Class::Pin => MessageClass::Pin,
            sc_chat_wire::msg::Class::Meta => MessageClass::Meta,
            sc_chat_wire::msg::Class::System => MessageClass::System,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Bold,
    Italic,
    Code,
    Spoiler,
    Link,
    Mention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub kind: SpanKind,
    pub start: u32,
    pub len: u32,
    pub url: Option<String>,
    pub user_id: Option<UserId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionGroup {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forwarded {
    pub user_id: UserId,
    pub name: String,
    pub sent_at_ms: Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemKind {
    Created,
    Joined,
    Left,
    Added,
    Removed,
    Kicked,
    Banned,
    Muted,
    Renamed,
    Handoff,
    Track,
    Ttl,
    Role,
    Deleted,
    Moderator,
    Suggest,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEvent {
    pub kind: SystemKind,
    pub actor: Option<UserId>,
    pub target: Option<UserId>,
    pub name: Option<String>,
    pub ttl_s: Option<u32>,
    pub until: Option<Millis>,
    pub track: Option<Box<Embed>>,
    #[serde(default)]
    pub role: Option<MemberRole>,
    #[serde(default)]
    pub on: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageBody {
    Text(RichText),
    System(SystemEvent),
    Deleted,
    Locked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub conversation_id: ConvId,
    pub seq: Option<Seq>,
    pub client_id: ClientId,
    pub sender: UserId,
    pub sender_device: Option<DeviceId>,
    pub mine: bool,
    pub ts: Millis,
    pub edited_at: Option<Millis>,
    pub expires_at: Option<Millis>,
    pub state: DeliveryState,
    pub body: MessageBody,
    pub reply_to: Option<Seq>,
    pub mentions: Vec<UserId>,
    pub mentions_me: bool,
    pub embeds: Vec<Embed>,
    pub forwarded_from: Option<Forwarded>,
    pub reactions: Vec<ReactionGroup>,
    pub pinned: bool,
    pub local_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTarget {
    Seq(Seq),
    Pending(ClientId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRef {
    pub conversation_id: ConvId,
    pub seq: Seq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendInput {
    pub conversation_id: ConvId,
    pub client_id: Option<ClientId>,
    pub text: RichText,
    pub reply_to: Option<Seq>,
    pub embeds: Vec<Embed>,
    pub forward: Option<MessageRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub before: Option<Seq>,
    pub after: Option<Seq>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub messages: Vec<Message>,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerMark {
    pub user_id: UserId,
    pub read_seq: Seq,
    pub delivered_seq: Seq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoneMessage {
    pub conversation_id: ConvId,
    pub seq: Seq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    pub conversation_id: Option<ConvId>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub conversation_id: ConvId,
    pub seq: Seq,
    pub ts: Millis,
    pub sender: UserId,
    pub snippet: String,
}
