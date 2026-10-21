use serde::{Deserialize, Serialize};

use crate::contracts::MemberRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    pub user_id: String,
    pub role: String,
    pub state: String,
    pub accepted: bool,
    pub joined_seq: i64,
    pub invited_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddMembers {
    pub users: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MemberRole>,
    #[serde(default)]
    pub unban: bool,
}
