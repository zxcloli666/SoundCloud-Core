use sc_chat::api::wire::{
    DeleteAnswer, MediaSelection, MediaUsage, MinePage, RestoreAnswer, UrlsAnswer,
};
use sc_chat::media::{self, queue};
use sc_chat::{ChatError, ChatResult, ConvId, MediaConfig, MineQuery};
use uuid::Uuid;

use crate::runtime::Chat;
use crate::types::{MediaInfo, OpenedMedia};

impl Chat {
    pub fn media_config(&self) -> MediaConfig {
        self.config().map(|config| config.media).unwrap_or_default()
    }

    pub async fn media_urls(
        &self,
        conversation_id: Option<ConvId>,
        ids: Vec<Uuid>,
    ) -> ChatResult<UrlsAnswer> {
        self.ready()?;
        queue::urls(&self.api, conversation_id, &ids).await
    }

    pub async fn media_info(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
    ) -> ChatResult<Option<MediaInfo>> {
        self.blocking(move |core| {
            Ok(core
                .store
                .attachment(conversation_id, media_id)?
                .as_ref()
                .and_then(MediaInfo::of))
        })
        .await
    }

    pub async fn open_image(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
        ciphertext: Vec<u8>,
    ) -> ChatResult<OpenedMedia> {
        let worker = self.media.clone();
        self.blocking(move |core| {
            let attachment = core
                .store
                .attachment(conversation_id, media_id)?
                .ok_or(ChatError::NotFound)?;
            Ok(media::open_image(&worker, &ciphertext, &attachment)?.into())
        })
        .await
    }

    pub async fn open_thumb(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
        ciphertext: Vec<u8>,
    ) -> ChatResult<OpenedMedia> {
        let worker = self.media.clone();
        self.blocking(move |core| {
            let attachment = core
                .store
                .attachment(conversation_id, media_id)?
                .ok_or(ChatError::NotFound)?;
            let thumb = attachment.thumb.as_ref().ok_or(ChatError::NotFound)?;
            Ok(media::open_thumb(&worker, &ciphertext, thumb)?.into())
        })
        .await
    }

    pub async fn media_usage(&self) -> ChatResult<MediaUsage> {
        self.ready()?;
        self.api.media_usage().await
    }

    pub async fn media_mine(&self, query: MineQuery) -> ChatResult<MinePage> {
        self.ready()?;
        self.api.media_mine(&query).await
    }

    pub async fn delete_media(&self, selection: MediaSelection) -> ChatResult<DeleteAnswer> {
        self.ready()?;
        self.api.delete_media(&selection).await
    }

    pub async fn restore_media(&self, selection: MediaSelection) -> ChatResult<RestoreAnswer> {
        self.ready()?;
        self.api.restore_media(&selection).await
    }
}
