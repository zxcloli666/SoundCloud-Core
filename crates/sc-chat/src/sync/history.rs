use super::SyncEngine;
use super::local::Local;
use crate::contracts::{ChatResult, ConvId, HistoryPage, HistoryQuery};

impl<L: Local> SyncEngine<L> {
    pub async fn fetch_history(
        &self,
        conversation_id: ConvId,
        query: &HistoryQuery,
    ) -> ChatResult<HistoryPage> {
        let page = self.api.history(conversation_id, query).await?;
        self.store_rows(conversation_id, &page.messages).await?;
        let mut local = self.store.messages(conversation_id, query)?;
        local.more = page.more;
        Ok(local)
    }
}
