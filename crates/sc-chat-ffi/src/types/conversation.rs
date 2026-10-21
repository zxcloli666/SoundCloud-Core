use sc_chat::{
    ConvId, ConversationKind, ConversationPage, ConversationSummary, CreateConversation,
    ListFilter, ListQuery, MemberRole, MemberState, Message, MessagePreview, Millis, NotifyLevel,
    PeerMark, PreviewKind, RichText, Seq, ThreadSnapshot, UserId, UserRef,
};

#[uniffi::remote(Enum)]
pub enum ConversationKind {
    Direct,
    Group,
    Room,
    Saved,
}

#[uniffi::remote(Enum)]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[uniffi::remote(Enum)]
pub enum MemberState {
    Active,
    Invited,
    Left,
    Kicked,
    Banned,
}

#[uniffi::remote(Enum)]
pub enum NotifyLevel {
    All,
    Mentions,
    None,
}

#[uniffi::remote(Record)]
pub struct UserRef {
    pub id: UserId,
    pub name: String,
    pub avatar_url: Option<String>,
    pub permalink: Option<String>,
}

#[uniffi::remote(Enum)]
pub enum PreviewKind {
    Text,
    Embed,
    System,
    Deleted,
    Locked,
    Unknown,
}

#[uniffi::remote(Record)]
pub struct MessagePreview {
    pub seq: Seq,
    pub sender: UserId,
    pub kind: PreviewKind,
    pub text: String,
    pub ts: Millis,
    pub expires_at: Option<Millis>,
}

#[uniffi::remote(Record)]
pub struct ConversationSummary {
    pub id: ConvId,
    pub kind: ConversationKind,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub peer: Option<UserRef>,
    pub role: MemberRole,
    pub state: MemberState,
    pub accepted: bool,
    pub member_count: u32,
    pub last_seq: Seq,
    pub last_at: Millis,
    pub last_read_seq: Seq,
    pub unread: u32,
    pub mentions: u32,
    pub preview: Option<MessagePreview>,
    pub muted_until: Option<Millis>,
    pub notify: NotifyLevel,
    pub pinned_at: Option<Millis>,
    pub archived: bool,
    pub ttl_s: Option<u32>,
    pub room_code: Option<String>,
    pub history_locked: bool,
    pub has_draft: bool,
}

#[uniffi::remote(Enum)]
pub enum ListFilter {
    All,
    Direct,
    Groups,
    Unread,
    Requests,
    Archived,
}

#[uniffi::remote(Record)]
pub struct ListQuery {
    pub filter: ListFilter,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[uniffi::remote(Record)]
pub struct ConversationPage {
    pub items: Vec<ConversationSummary>,
    pub next_cursor: Option<String>,
}

#[uniffi::remote(Record)]
pub struct CreateConversation {
    pub kind: ConversationKind,
    pub members: Vec<UserId>,
    pub title: Option<String>,
}

#[uniffi::remote(Record)]
pub struct ThreadSnapshot {
    pub conversation: ConversationSummary,
    pub messages: Vec<Message>,
    pub more_before: bool,
    pub marks: Vec<PeerMark>,
    pub pins: Vec<Seq>,
    pub typing: Vec<UserId>,
    pub draft: Option<RichText>,
}
