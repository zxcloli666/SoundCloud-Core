use sc_chat::api::wire::{
    CommitAnswer, DeleteAnswer, MediaSelection, MediaUsage, MinePage, RestoreAnswer, UrlsAnswer,
};
use sc_chat::{
    AvatarChange, BackdropChange, BackdropReport, ChatError, ConvId, MediaConfig, MediaKind,
    Message, MineQuery, SendInput, UploadObject,
};
use uuid::Uuid;

use crate::client::ChatClient;
use crate::types::{
    MediaInfo, MediaUploadRow, NewMediaUpload, OpenedMedia, PreparedMedia, PutTargets,
};

#[uniffi::export]
impl ChatClient {
    pub fn media_config(&self) -> MediaConfig {
        self.chat.media_config()
    }

    pub async fn prepare_image(&self, source: Vec<u8>) -> Result<PreparedMedia, ChatError> {
        self.run(|chat| async move { chat.prepare(MediaKind::Image, source).await })
            .await
    }

    pub async fn prepare_avatar(&self, source: Vec<u8>) -> Result<PreparedMedia, ChatError> {
        self.run(|chat| async move { chat.prepare(MediaKind::Avatar, source).await })
            .await
    }

    pub async fn prepare_backdrop(&self, source: Vec<u8>) -> Result<PreparedMedia, ChatError> {
        self.run(|chat| async move { chat.prepare(MediaKind::Backdrop, source).await })
            .await
    }

    pub async fn enqueue_upload(
        &self,
        upload: NewMediaUpload,
    ) -> Result<MediaUploadRow, ChatError> {
        self.run(|chat| async move { chat.enqueue_upload(upload).await })
            .await
    }

    pub async fn reserve_upload(&self, id: Uuid) -> Result<PutTargets, ChatError> {
        self.run(|chat| async move { chat.reserve_upload(id).await })
            .await
    }

    pub async fn upload_progress(
        &self,
        id: Uuid,
        object: UploadObject,
        parts_sent: u32,
        stage_id: Option<String>,
    ) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.upload_progress(id, object, parts_sent, stage_id).await })
            .await
    }

    pub async fn upload_object_done(
        &self,
        id: Uuid,
        object: UploadObject,
    ) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.upload_object_done(id, object).await })
            .await
    }

    pub async fn upload_redo(&self, id: Uuid, object: UploadObject) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.upload_redo(id, object).await })
            .await
    }

    pub async fn commit_upload(&self, id: Uuid) -> Result<CommitAnswer, ChatError> {
        self.run(|chat| async move { chat.commit_upload(id).await })
            .await
    }

    pub async fn upload_failed(&self, id: Uuid, retryable: bool) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.upload_failed(id, retryable).await })
            .await
    }

    pub async fn upload_restart(&self, id: Uuid) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.upload_restart(id).await })
            .await
    }

    pub async fn remove_album_uploads(
        &self,
        album: Uuid,
    ) -> Result<Vec<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.remove_album_uploads(album).await })
            .await
    }

    pub async fn remove_upload(&self, id: Uuid) -> Result<Option<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.remove_upload(id).await })
            .await
    }

    pub async fn upload(&self, id: Uuid) -> Result<Option<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.upload(id).await }).await
    }

    pub async fn uploads(&self) -> Result<Vec<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.uploads().await }).await
    }

    pub async fn album_uploads(&self, album: Uuid) -> Result<Vec<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.album_uploads(album).await })
            .await
    }

    pub async fn next_upload(&self) -> Result<Option<MediaUploadRow>, ChatError> {
        self.run(|chat| async move { chat.next_upload().await })
            .await
    }

    pub async fn send_album(&self, input: SendInput, album: Uuid) -> Result<Message, ChatError> {
        self.run(|chat| async move { chat.send_album(input, album).await })
            .await
    }

    pub async fn media_urls(
        &self,
        conversation_id: Option<ConvId>,
        ids: Vec<Uuid>,
    ) -> Result<UrlsAnswer, ChatError> {
        self.run(|chat| async move { chat.media_urls(conversation_id, ids).await })
            .await
    }

    pub async fn media_info(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
    ) -> Result<Option<MediaInfo>, ChatError> {
        self.run(|chat| async move { chat.media_info(conversation_id, media_id).await })
            .await
    }

    pub async fn open_image(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
        ciphertext: Vec<u8>,
    ) -> Result<OpenedMedia, ChatError> {
        self.run(|chat| async move { chat.open_image(conversation_id, media_id, ciphertext).await })
            .await
    }

    pub async fn open_thumb(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
        ciphertext: Vec<u8>,
    ) -> Result<OpenedMedia, ChatError> {
        self.run(|chat| async move { chat.open_thumb(conversation_id, media_id, ciphertext).await })
            .await
    }

    pub async fn media_usage(&self) -> Result<MediaUsage, ChatError> {
        self.run(|chat| async move { chat.media_usage().await })
            .await
    }

    pub async fn media_mine(&self, query: MineQuery) -> Result<MinePage, ChatError> {
        self.run(|chat| async move { chat.media_mine(query).await })
            .await
    }

    pub async fn delete_media(&self, selection: MediaSelection) -> Result<DeleteAnswer, ChatError> {
        self.run(|chat| async move { chat.delete_media(selection).await })
            .await
    }

    pub async fn restore_media(
        &self,
        selection: MediaSelection,
    ) -> Result<RestoreAnswer, ChatError> {
        self.run(|chat| async move { chat.restore_media(selection).await })
            .await
    }

    pub async fn set_group_avatar(
        &self,
        conversation_id: ConvId,
        change: AvatarChange,
    ) -> Result<Message, ChatError> {
        self.run(|chat| async move { chat.set_group_avatar(conversation_id, change).await })
            .await
    }

    pub async fn report_backdrop(&self, report: BackdropReport) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.report_backdrop(report).await })
            .await
    }

    pub async fn set_backdrop(
        &self,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> Result<Option<Message>, ChatError> {
        self.run(|chat| async move { chat.set_backdrop(conversation_id, change).await })
            .await
    }

    pub async fn set_backdrop_preset(
        &self,
        conversation_id: ConvId,
        preset_id: String,
    ) -> Result<Option<Message>, ChatError> {
        self.run(|chat| async move { chat.set_backdrop_preset(conversation_id, preset_id).await })
            .await
    }

    pub async fn set_room_backdrop(
        &self,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> Result<Message, ChatError> {
        self.run(|chat| async move { chat.set_room_backdrop(conversation_id, change).await })
            .await
    }

    pub async fn set_room_preset(
        &self,
        conversation_id: ConvId,
        preset_id: String,
    ) -> Result<Option<Message>, ChatError> {
        self.run(|chat| async move { chat.set_room_preset(conversation_id, preset_id).await })
            .await
    }
}
