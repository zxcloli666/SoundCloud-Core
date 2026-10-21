use sc_chat::{
    ClientId, ConvId, DeliveryState, DeviceId, Embed, Forwarded, GoneMessage, HistoryPage,
    HistoryQuery, MemberRole, Message, MessageBody, MessageRef, Millis, PeerMark, ReactionGroup,
    RichText, SendInput, Seq, Span, SpanKind, SystemEvent, SystemKind, UserId,
};

#[uniffi::remote(Enum)]
pub enum DeliveryState {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

#[uniffi::remote(Enum)]
pub enum SpanKind {
    Bold,
    Italic,
    Code,
    Spoiler,
    Link,
    Mention,
}

#[uniffi::remote(Record)]
pub struct Span {
    pub kind: SpanKind,
    pub start: u32,
    pub len: u32,
    pub url: Option<String>,
    pub user_id: Option<UserId>,
}

#[uniffi::remote(Record)]
pub struct RichText {
    pub text: String,
    pub spans: Vec<Span>,
}

#[uniffi::remote(Record)]
pub struct ReactionGroup {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

#[uniffi::remote(Record)]
pub struct Forwarded {
    pub user_id: UserId,
    pub name: String,
    pub sent_at_ms: Millis,
}

#[uniffi::remote(Enum)]
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

#[uniffi::remote(Record)]
pub struct SystemEvent {
    pub kind: SystemKind,
    pub actor: Option<UserId>,
    pub target: Option<UserId>,
    pub name: Option<String>,
    pub ttl_s: Option<u32>,
    pub until: Option<Millis>,
    pub track: Option<Box<Embed>>,
    pub role: Option<MemberRole>,
    pub on: Option<bool>,
}

#[uniffi::remote(Enum)]
pub enum MessageBody {
    Text(RichText),
    System(SystemEvent),
    Deleted,
    Locked,
    Unknown,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
pub struct MessageRef {
    pub conversation_id: ConvId,
    pub seq: Seq,
}

#[uniffi::remote(Record)]
pub struct SendInput {
    pub conversation_id: ConvId,
    pub client_id: Option<ClientId>,
    pub text: RichText,
    pub reply_to: Option<Seq>,
    pub embeds: Vec<Embed>,
    pub forward: Option<MessageRef>,
}

#[uniffi::remote(Record)]
pub struct HistoryQuery {
    pub before: Option<Seq>,
    pub after: Option<Seq>,
    pub limit: u32,
}

#[uniffi::remote(Record)]
pub struct HistoryPage {
    pub messages: Vec<Message>,
    pub more: bool,
}

#[uniffi::remote(Record)]
pub struct PeerMark {
    pub user_id: UserId,
    pub read_seq: Seq,
    pub delivered_seq: Seq,
}

#[uniffi::remote(Record)]
pub struct GoneMessage {
    pub conversation_id: ConvId,
    pub seq: Seq,
}
