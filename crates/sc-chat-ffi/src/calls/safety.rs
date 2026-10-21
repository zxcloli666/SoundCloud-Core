use sc_chat::api::wire::FileReport;
use sc_chat::{
    ChatError, ChatResult, ConvId, ConversationSummary, ReportInput, is_disappearing_ttl,
};

use crate::runtime::Chat;

const COMMENT_MAX: usize = 500;

impl Chat {
    pub async fn set_ttl(
        &self,
        conversation_id: ConvId,
        ttl_s: Option<u32>,
    ) -> ChatResult<ConversationSummary> {
        if ttl_s.is_some_and(|ttl| !is_disappearing_ttl(ttl)) {
            return Err(ChatError::Invalid);
        }
        let core = self.ready()?;
        self.api.set_ttl(conversation_id, ttl_s).await?;
        core.sync.kick();
        core.sync.refresh_conversation(conversation_id).await
    }

    pub async fn report(&self, report: ReportInput) -> ChatResult<()> {
        let report = checked(report)?;
        self.api.report(&FileReport::from(&report)).await?;
        Ok(())
    }
}

pub fn checked(report: ReportInput) -> ChatResult<ReportInput> {
    let comment = report
        .comment
        .map(|comment| comment.trim().to_string())
        .filter(|comment| !comment.is_empty());
    let too_long = comment
        .as_ref()
        .is_some_and(|comment| comment.chars().count() > COMMENT_MAX);
    if report.seq_from > report.seq_to || too_long {
        return Err(ChatError::Invalid);
    }
    Ok(ReportInput { comment, ..report })
}

#[cfg(test)]
mod tests {
    use sc_chat::ReportReason;

    use super::*;

    fn report(seq_from: i64, seq_to: i64, comment: Option<&str>) -> ReportInput {
        ReportInput {
            conversation_id: uuid::Uuid::nil(),
            seq_from,
            seq_to,
            reason: ReportReason::Spam,
            comment: comment.map(str::to_owned),
        }
    }

    #[test]
    fn a_blank_comment_is_dropped() {
        let checked = checked(report(1, 3, Some("   "))).unwrap();
        assert_eq!(checked.comment, None);
    }

    #[test]
    fn a_reversed_range_or_a_long_comment_is_refused() {
        assert_eq!(checked(report(5, 2, None)).err(), Some(ChatError::Invalid));
        let long = "x".repeat(COMMENT_MAX + 1);
        assert_eq!(
            checked(report(1, 1, Some(&long))).err(),
            Some(ChatError::Invalid)
        );
    }
}
