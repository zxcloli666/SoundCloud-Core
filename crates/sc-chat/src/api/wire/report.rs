use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::contracts::{ReportInput, ReportReason};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReport {
    pub conversation_id: Uuid,
    pub seq_from: i64,
    pub seq_to: i64,
    pub reason: String,
    pub comment: Option<String>,
}

impl From<&ReportInput> for FileReport {
    fn from(input: &ReportInput) -> Self {
        Self {
            conversation_id: input.conversation_id,
            seq_from: input.seq_from,
            seq_to: input.seq_to,
            reason: reason_name(input.reason).to_owned(),
            comment: input.comment.clone(),
        }
    }
}

fn reason_name(reason: ReportReason) -> &'static str {
    match reason {
        ReportReason::Spam => "spam",
        ReportReason::Harassment => "harassment",
        ReportReason::Doxxing => "doxxing",
        ReportReason::Illegal => "illegal",
        ReportReason::Other => "other",
    }
}
