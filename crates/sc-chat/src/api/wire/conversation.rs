use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::contracts::{ConversationKind, Mute, MyPatch, NotifyLevel};

pub const MUTE_FOREVER_MS: i64 = 253_402_300_799_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationView {
    pub id: Uuid,
    pub kind: ConversationKind,
    pub peer: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub last_seq: i64,
    pub last_at: i64,
    pub key_epoch: i32,
    #[serde(default, with = "super::b64::option")]
    pub meta_ct: Option<Vec<u8>>,
    pub meta_epoch: Option<i32>,
    #[serde(default)]
    pub settings: Value,
    pub member_count: i32,
    pub ttl_s: Option<u32>,
    pub room_ref: Option<String>,
    pub me: MembershipView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipView {
    pub role: String,
    pub state: String,
    pub accepted: bool,
    pub joined_seq: i64,
    pub joined_epoch: i32,
    pub last_read_seq: i64,
    pub muted_until: Option<i64>,
    pub notify: String,
    pub pinned_at: Option<i64>,
    pub archived: bool,
    pub hidden_before_seq: i64,
    pub invited_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationList {
    pub items: Vec<ConversationView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewConversation {
    pub kind: ConversationKind,
    pub members: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "super::b64::option"
    )]
    pub meta_ct: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_epoch: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationChange {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "super::b64::option"
    )]
    pub meta_ct: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_epoch: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<GroupSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admins_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_visible: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MyChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted_until: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify: Option<NotifyLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

impl From<&MyPatch> for MyChange {
    fn from(patch: &MyPatch) -> Self {
        Self {
            muted_until: patch.mute.map(|mute| match mute {
                Mute::Off => None,
                Mute::Until { at } => Some(at),
                Mute::Forever => Some(MUTE_FOREVER_MS),
            }),
            notify: patch.notify,
            pinned: patch.pinned,
            archived: patch.archived,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtlAnswer {
    pub ttl_s: Option<u32>,
    pub seq: Option<i64>,
}
