use uuid::Uuid;

use crate::api::Api;
use crate::api::path::query;
use crate::api::wire::{
    CommitAnswer, DeleteAnswer, MediaSelection, MediaUsage, MinePage, ReserveAnswer, ReserveMedia,
    RestoreAnswer, UrlsAnswer, UrlsRequest,
};
use crate::contracts::{ChatResult, MineOrder, MineQuery};

impl Api {
    pub async fn reserve_media(&self, body: &ReserveMedia) -> ChatResult<ReserveAnswer> {
        self.post("/chat/media/reserve", body).await
    }

    pub async fn commit_media(&self, id: Uuid) -> ChatResult<CommitAnswer> {
        self.post_empty(&format!("/chat/media/{id}/commit")).await
    }

    pub async fn media_urls(&self, body: &UrlsRequest) -> ChatResult<UrlsAnswer> {
        self.post("/chat/media/urls", body).await
    }

    pub async fn media_usage(&self) -> ChatResult<MediaUsage> {
        self.get("/chat/media/usage", "").await
    }

    pub async fn media_mine(&self, page: &MineQuery) -> ChatResult<MinePage> {
        let order = match page.order {
            MineOrder::New => "new",
            MineOrder::Old => "old",
            MineOrder::Big => "big",
        };
        let query = query(&[
            ("order", Some(order.to_owned())),
            (
                "conversation_id",
                page.conversation_id.map(|id| id.to_string()),
            ),
            ("cursor", page.cursor.clone()),
            ("limit", (page.limit > 0).then(|| page.limit.to_string())),
        ]);
        self.get("/chat/media/mine", &query).await
    }

    pub async fn delete_media(&self, selection: &MediaSelection) -> ChatResult<DeleteAnswer> {
        self.post("/chat/media/delete", selection).await
    }

    pub async fn restore_media(&self, selection: &MediaSelection) -> ChatResult<RestoreAnswer> {
        self.post("/chat/media/restore", selection).await
    }
}
