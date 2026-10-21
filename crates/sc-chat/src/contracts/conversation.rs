use serde::{Deserialize, Serialize};

use super::ids::{ConvId, Millis, Seq, UserId};
use super::message::{Message, PeerMark, RichText};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Direct,
    Group,
    Room,
    #[serde(rename = "self")]
    Saved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberState {
    Active,
    Invited,
    Left,
    Kicked,
    Banned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyLevel {
    All,
    Mentions,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRef {
    pub id: UserId,
    pub name: String,
    pub avatar_url: Option<String>,
    pub permalink: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewKind {
    Text,
    Embed,
    System,
    Deleted,
    Locked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePreview {
    pub seq: Seq,
    pub sender: UserId,
    pub kind: PreviewKind,
    pub text: String,
    pub ts: Millis,
    pub expires_at: Option<Millis>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListFilter {
    All,
    Direct,
    Groups,
    Unread,
    Requests,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub filter: ListFilter,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPage {
    pub items: Vec<ConversationSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversation {
    pub kind: ConversationKind,
    pub members: Vec<UserId>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMeta {
    pub title: String,
    pub description: String,
    pub avatar_media_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaPatch {
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mute {
    Off,
    Until { at: Millis },
    Forever,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyPatch {
    pub mute: Option<Mute>,
    pub notify: Option<NotifyLevel>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationPatch {
    pub meta: Option<MetaPatch>,
    pub me: Option<MyPatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteScope {
    Me,
    Everyone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub user: UserRef,
    pub role: MemberRole,
    pub state: MemberState,
    pub accepted: bool,
    pub joined_seq: Seq,
    pub invited_by: Option<UserId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub conversation: ConversationSummary,
    pub from: UserRef,
    pub messages: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteOptions {
    pub expires_in_s: Option<u32>,
    pub uses: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub token: String,
    pub url: String,
    pub expires_at: Millis,
    pub uses_left: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSnapshot {
    pub conversation: ConversationSummary,
    pub messages: Vec<Message>,
    pub more_before: bool,
    pub marks: Vec<PeerMark>,
    pub pins: Vec<Seq>,
    pub typing: Vec<UserId>,
    pub draft: Option<RichText>,
}
