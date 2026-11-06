use super::SyncEngine;
use super::local::Local;
use crate::contracts::{ChatResult, ConversationSummary};

impl<L: Local> SyncEngine<L> {
    pub async fn open_public(&self) -> ChatResult<(ConversationSummary, u32)> {
        let opened = self.api.open_public().await?;
        let summary = self.adopt(&opened.conversation).await?;
        Ok((summary, opened.online))
    }
}
