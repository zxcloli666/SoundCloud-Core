use crate::api::Api;
use crate::api::path::segment;
use crate::api::wire::{
    CardView, FileReport, FiledReport, InviteView, JoinAnswer, NewInvite, RequestList,
    SettingsChange, SettingsView,
};
use crate::contracts::{ChatResult, ConvId};

impl Api {
    pub async fn requests(&self) -> ChatResult<RequestList> {
        self.get("/chat/requests", "").await
    }

    pub async fn create_invite(&self, id: ConvId, body: &NewInvite) -> ChatResult<InviteView> {
        self.post(&format!("/chat/conversations/{id}/invites"), body)
            .await
    }

    pub async fn revoke_invite(&self, token: &str) -> ChatResult<()> {
        self.delete(&format!("/chat/invites/{}", segment(token)), "")
            .await
    }

    pub async fn join_invite(&self, token: &str) -> ChatResult<JoinAnswer> {
        self.post_empty(&format!("/chat/invites/{}/join", segment(token)))
            .await
    }

    pub async fn settings(&self) -> ChatResult<SettingsView> {
        self.get("/chat/settings", "").await
    }

    pub async fn put_settings(&self, change: &SettingsChange) -> ChatResult<SettingsView> {
        self.put("/chat/settings", change).await
    }

    pub async fn block(&self, user: &str) -> ChatResult<()> {
        self.put_empty(&format!("/chat/blocks/{}", segment(user)))
            .await
    }

    pub async fn unblock(&self, user: &str) -> ChatResult<()> {
        self.delete(&format!("/chat/blocks/{}", segment(user)), "")
            .await
    }

    pub async fn report(&self, body: &FileReport) -> ChatResult<FiledReport> {
        self.post("/chat/reports", body).await
    }

    pub async fn user_card(&self, user: &str) -> ChatResult<CardView> {
        self.get(&format!("/chat/users/{}/card", segment(user)), "")
            .await
    }
}
