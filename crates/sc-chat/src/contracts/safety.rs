use serde::{Deserialize, Serialize};

use super::conversation::UserRef;
use super::embed::Embed;
use super::ids::{ConvId, Millis, Seq};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    Spam,
    Harassment,
    Doxxing,
    Illegal,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportInput {
    pub conversation_id: ConvId,
    pub seq_from: Seq,
    pub seq_to: Seq,
    pub reason: ReportReason,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmPolicy {
    Everyone,
    Following,
    Nobody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSettings {
    pub dm_policy: DmPolicy,
    pub read_receipts: bool,
    pub typing: bool,
    pub presence_share: bool,
    pub blocked: Vec<UserRef>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub dm_policy: Option<DmPolicy>,
    pub read_receipts: Option<bool>,
    pub typing: Option<bool>,
    pub presence_share: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub online: bool,
    pub last_seen_at: Option<Millis>,
    pub listening: Option<Embed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCard {
    pub user: UserRef,
    pub contact: bool,
    pub blocked: bool,
    pub can_message: bool,
    pub presence: Option<Presence>,
}
