use crate::api::Api;
use crate::api::path::query;
use crate::api::wire::{
    ConversationChange, ConversationList, ConversationView, MyChange, NewConversation, SetTtl,
    TtlAnswer,
};
use crate::contracts::{ChatResult, ConvId, DeleteScope};

impl Api {
    pub async fn conversations(&self, cursor: Option<&str>) -> ChatResult<ConversationList> {
        let query = query(&[("cursor", cursor.map(str::to_owned))]);
        self.get("/chat/conversations", &query).await
    }

    pub async fn create_conversation(
        &self,
        body: &NewConversation,
    ) -> ChatResult<ConversationView> {
        self.post("/chat/conversations", body).await
    }

    pub async fn conversation(&self, id: ConvId) -> ChatResult<ConversationView> {
        self.get(&format!("/chat/conversations/{id}"), "").await
    }

    pub async fn update_conversation(
        &self,
        id: ConvId,
        change: &ConversationChange,
    ) -> ChatResult<ConversationView> {
        self.patch(&format!("/chat/conversations/{id}"), change)
            .await
    }

    pub async fn delete_conversation(&self, id: ConvId, scope: DeleteScope) -> ChatResult<()> {
        let scope = match scope {
            DeleteScope::Me => "me",
            DeleteScope::Everyone => "everyone",
        };
        self.delete(
            &format!("/chat/conversations/{id}"),
            &format!("for={scope}"),
        )
        .await
    }

    pub async fn update_me(&self, id: ConvId, change: &MyChange) -> ChatResult<ConversationView> {
        self.put(&format!("/chat/conversations/{id}/me"), change)
            .await
    }

    pub async fn set_ttl(&self, id: ConvId, ttl_s: Option<u32>) -> ChatResult<TtlAnswer> {
        self.put(&format!("/chat/conversations/{id}/ttl"), &SetTtl { ttl_s })
            .await
    }
}
