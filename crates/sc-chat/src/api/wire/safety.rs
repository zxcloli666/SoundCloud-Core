use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::path::segment;
use crate::contracts::{
    ChatSettings, ConversationKind, DmPolicy, Invite, SettingsPatch, UserCard, UserRef,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestView {
    pub conversation_id: Uuid,
    pub kind: ConversationKind,
    pub from: String,
    pub since: i64,
    pub last_seq: i64,
    pub last_at: i64,
    pub member_count: i32,
    #[serde(default, with = "super::b64::option")]
    pub meta_ct: Option<Vec<u8>>,
    pub meta_epoch: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestList {
    pub requests: Vec<RequestView>,
    pub count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_s: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteView {
    pub token: String,
    pub conversation_id: Uuid,
    pub expires_at: i64,
    pub uses_left: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinAnswer {
    pub conversation_id: Uuid,
    pub joined: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsView {
    pub dm_policy: DmPolicy,
    pub read_receipts: bool,
    pub typing: bool,
    pub presence_share: bool,
    pub restricted_until: Option<i64>,
    #[serde(default)]
    pub restriction_scope: Vec<String>,
    #[serde(default)]
    pub blocked: Vec<BlockedView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedView {
    pub user_id: String,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_policy: Option<DmPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_receipts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_share: Option<bool>,
}

impl From<&SettingsPatch> for SettingsChange {
    fn from(patch: &SettingsPatch) -> Self {
        Self {
            dm_policy: patch.dm_policy,
            read_receipts: patch.read_receipts,
            typing: patch.typing,
            presence_share: patch.presence_share,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardView {
    pub user_id: String,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub verified: bool,
    pub blocked: bool,
    pub contact: bool,
    pub can_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiledReport {
    pub id: Uuid,
    pub state: String,
    #[serde(default)]
    pub duplicate: bool,
}

pub const INVITE_LINK: &str = "scd://invite?token=";

const CANNOT_MESSAGE: &str = "unavailable";

impl From<&InviteView> for Invite {
    fn from(view: &InviteView) -> Self {
        Invite {
            token: view.token.clone(),
            url: format!("{INVITE_LINK}{}", segment(&view.token)),
            expires_at: view.expires_at,
            uses_left: view.uses_left,
        }
    }
}

impl From<&BlockedView> for UserRef {
    fn from(view: &BlockedView) -> Self {
        UserRef {
            id: view.user_id.clone(),
            name: view.username.clone().unwrap_or_default(),
            avatar_url: view.avatar_url.clone(),
            permalink: None,
        }
    }
}

impl From<&SettingsView> for ChatSettings {
    fn from(view: &SettingsView) -> Self {
        ChatSettings {
            dm_policy: view.dm_policy,
            read_receipts: view.read_receipts,
            typing: view.typing,
            presence_share: view.presence_share,
            blocked: view.blocked.iter().map(UserRef::from).collect(),
        }
    }
}

impl From<&CardView> for UserCard {
    fn from(view: &CardView) -> Self {
        UserCard {
            user: UserRef {
                id: view.user_id.clone(),
                name: view.username.clone().unwrap_or_default(),
                avatar_url: view.avatar_url.clone(),
                permalink: view.permalink_url.clone(),
            },
            contact: view.contact,
            blocked: view.blocked,
            can_message: view.can_message != CANNOT_MESSAGE,
            presence: None,
        }
    }
}
