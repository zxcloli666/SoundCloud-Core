use crate::api::Api;
use crate::api::path::{query, segment};
use crate::api::wire::{AddMembers, MemberChange, MemberView};
use crate::contracts::{ChatResult, ConvId};

impl Api {
    pub async fn members(&self, id: ConvId) -> ChatResult<Vec<MemberView>> {
        self.get(&format!("/chat/conversations/{id}/members"), "")
            .await
    }

    pub async fn add_members(&self, id: ConvId, users: &[String]) -> ChatResult<Vec<String>> {
        let body = AddMembers {
            users: users.to_vec(),
        };
        self.post(&format!("/chat/conversations/{id}/members"), &body)
            .await
    }

    pub async fn remove_member(&self, id: ConvId, user: &str, ban: bool) -> ChatResult<()> {
        let query = query(&[("ban", ban.then(|| "true".to_owned()))]);
        self.delete(&member_path(id, user), &query).await
    }

    pub async fn update_member(
        &self,
        id: ConvId,
        user: &str,
        change: &MemberChange,
    ) -> ChatResult<Vec<MemberView>> {
        self.put(&member_path(id, user), change).await
    }

    pub async fn accept(&self, id: ConvId) -> ChatResult<()> {
        self.post_empty(&format!("/chat/conversations/{id}/accept"))
            .await
    }

    pub async fn decline(&self, id: ConvId) -> ChatResult<()> {
        self.post_empty(&format!("/chat/conversations/{id}/decline"))
            .await
    }
}

fn member_path(id: ConvId, user: &str) -> String {
    format!("/chat/conversations/{id}/members/{}", segment(user))
}
