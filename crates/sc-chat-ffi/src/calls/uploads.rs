use sc_chat::api::wire::CommitAnswer;
use sc_chat::media::{self, queue};
use sc_chat::store::NewUpload;
use sc_chat::{ChatError, ChatResult, MediaKind, UploadObject};
use uuid::Uuid;

use crate::runtime::Chat;
use crate::types::{MediaUploadRow, NewMediaUpload, PreparedMedia, PutTargets};

impl Chat {
    pub async fn prepare(&self, kind: MediaKind, source: Vec<u8>) -> ChatResult<PreparedMedia> {
        let slot = self
            .preparing
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ChatError::Internal)?;
        self.handle
            .spawn_blocking(move || {
                let _slot = slot;
                prepared(kind, &source)
            })
            .await
            .unwrap_or(Err(ChatError::Internal))
    }

    pub async fn enqueue_upload(&self, upload: NewMediaUpload) -> ChatResult<MediaUploadRow> {
        let now = self.now_ms();
        self.blocking(move |core| {
            let new = NewUpload {
                album: upload.album,
                position: upload.position,
                conversation_id: upload.conversation_id,
                kind: upload.seal.kind,
                image_ref: upload.image_ref,
                thumb_ref: upload.thumb_ref,
                attachment: upload.seal.attachment.clone(),
            };
            let id = core.store.enqueue_upload(&new, now)?;
            row(core.store.upload(id)?)
        })
        .await
    }

    pub async fn reserve_upload(&self, id: Uuid) -> ChatResult<PutTargets> {
        let core = self.ready()?;
        let targets = queue::reserve(&self.api, &core.store, id, self.now_ms()).await?;
        Ok(targets.into())
    }

    pub async fn upload_progress(
        &self,
        id: Uuid,
        object: UploadObject,
        parts_sent: u32,
        stage_id: Option<String>,
    ) -> ChatResult<()> {
        let now = self.now_ms();
        self.blocking(move |core| {
            core.store
                .upload_progress(id, object, parts_sent, stage_id.as_deref(), now)
        })
        .await
    }

    pub async fn upload_object_done(&self, id: Uuid, object: UploadObject) -> ChatResult<()> {
        let now = self.now_ms();
        self.blocking(move |core| core.store.upload_object_done(id, object, now))
            .await
    }

    pub async fn upload_redo(&self, id: Uuid, object: UploadObject) -> ChatResult<()> {
        let now = self.now_ms();
        self.blocking(move |core| core.store.upload_redo(id, object, now))
            .await
    }

    pub async fn commit_upload(&self, id: Uuid) -> ChatResult<CommitAnswer> {
        let core = self.ready()?;
        queue::commit(&self.api, &core.store, id, self.now_ms()).await
    }

    pub async fn upload_failed(&self, id: Uuid, retryable: bool) -> ChatResult<()> {
        let now = self.now_ms();
        self.blocking(move |core| core.store.upload_failed(id, retryable, now))
            .await
    }

    pub async fn upload_restart(&self, id: Uuid) -> ChatResult<()> {
        let now = self.now_ms();
        self.blocking(move |core| core.store.upload_restart(id, now))
            .await
    }

    pub async fn remove_album_uploads(&self, album: Uuid) -> ChatResult<Vec<MediaUploadRow>> {
        self.blocking(move |core| rows(core.store.remove_album_uploads(album)?))
            .await
    }

    pub async fn remove_upload(&self, id: Uuid) -> ChatResult<Option<MediaUploadRow>> {
        self.blocking(move |core| Ok(core.store.remove_upload(id)?.map(MediaUploadRow::from)))
            .await
    }

    pub async fn upload(&self, id: Uuid) -> ChatResult<Option<MediaUploadRow>> {
        self.blocking(move |core| Ok(core.store.upload(id)?.map(MediaUploadRow::from)))
            .await
    }

    pub async fn uploads(&self) -> ChatResult<Vec<MediaUploadRow>> {
        self.blocking(|core| rows(core.store.uploads()?)).await
    }

    pub async fn album_uploads(&self, album: Uuid) -> ChatResult<Vec<MediaUploadRow>> {
        self.blocking(move |core| rows(core.store.album_uploads(album)?))
            .await
    }

    pub async fn next_upload(&self) -> ChatResult<Option<MediaUploadRow>> {
        self.blocking(|core| Ok(core.store.next_upload()?.map(MediaUploadRow::from)))
            .await
    }
}

fn prepared(kind: MediaKind, source: &[u8]) -> ChatResult<PreparedMedia> {
    Ok(match kind {
        MediaKind::Image => {
            let image = media::prepare_image(source)?;
            PreparedMedia::new(kind, image.image, Some(image.thumb), image.attachment)
        }
        MediaKind::Avatar => {
            let avatar = media::prepare_avatar(source)?;
            PreparedMedia::new(kind, avatar.image, None, avatar.attachment)
        }
        MediaKind::Backdrop => {
            let backdrop = media::prepare_backdrop(source)?;
            PreparedMedia::new(kind, backdrop.image, None, backdrop.attachment)
        }
    })
}

fn row(upload: Option<sc_chat::store::MediaUpload>) -> ChatResult<MediaUploadRow> {
    upload.map(MediaUploadRow::from).ok_or(ChatError::Store)
}

fn rows(uploads: Vec<sc_chat::store::MediaUpload>) -> ChatResult<Vec<MediaUploadRow>> {
    Ok(uploads.into_iter().map(MediaUploadRow::from).collect())
}
