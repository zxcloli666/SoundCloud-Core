use crate::api::Api;
use crate::api::path::query;
use crate::api::wire::{MessagesPage, SendAnswer, SendMessage};
use crate::contracts::{ChatResult, ConvId, HistoryQuery, Seq};

impl Api {
    pub async fn send(
        &self,
        conversation_id: ConvId,
        body: &SendMessage,
    ) -> ChatResult<SendAnswer> {
        self.post(
            &format!("/chat/conversations/{conversation_id}/messages"),
            body,
        )
        .await
    }

    pub async fn history(
        &self,
        conversation_id: ConvId,
        page: &HistoryQuery,
    ) -> ChatResult<MessagesPage> {
        let query = query(&[
            ("after", page.after.map(|seq| seq.to_string())),
            ("before", page.before.map(|seq| seq.to_string())),
            ("limit", (page.limit > 0).then(|| page.limit.to_string())),
        ]);
        self.get(
            &format!("/chat/conversations/{conversation_id}/messages"),
            &query,
        )
        .await
    }

    pub async fn delete_message(&self, conversation_id: ConvId, seq: Seq) -> ChatResult<()> {
        self.delete(
            &format!("/chat/conversations/{conversation_id}/messages/{seq}"),
            "for=everyone",
        )
        .await
    }
}
