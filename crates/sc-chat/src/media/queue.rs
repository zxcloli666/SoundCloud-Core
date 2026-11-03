use uuid::Uuid;

use super::URLS_MAX;
use crate::api::Api;
use crate::api::wire::{
    CommitAnswer, MediaKind, PresignedPut, ReserveMedia, UrlsAnswer, UrlsRequest,
};
use crate::contracts::{ChatError, ChatResult, ConvId, Millis};
use crate::store::{MediaUpload, Reserved, Store, UploadObject};

pub const PRESIGN_MARGIN_MS: Millis = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutTargets {
    pub expires_at: Millis,
    pub image: PresignedPut,
    pub thumb: PresignedPut,
}

pub async fn reserve(api: &Api, store: &Store, id: Uuid, now: Millis) -> ChatResult<PutTargets> {
    let upload = store.upload(id)?.ok_or(ChatError::NotFound)?;
    let answer = api.reserve_media(&reserve_request(&upload)?).await?;
    let thumb = answer.thumb.ok_or(ChatError::Protocol)?;
    let expires_at = now.saturating_add(i64::from(answer.expires_in_s) * 1_000 - PRESIGN_MARGIN_MS);
    let reserved = Reserved {
        media_id: answer.id,
        thumb_media_id: thumb.id,
        presign_expires_at: expires_at,
    };
    store.upload_reserved(id, &reserved, now)?;
    Ok(PutTargets {
        expires_at,
        image: answer.put,
        thumb: thumb.put,
    })
}

pub fn reserve_request(upload: &MediaUpload) -> ChatResult<ReserveMedia> {
    let thumb = upload.attachment.thumb.as_ref().ok_or(ChatError::Invalid)?;
    Ok(ReserveMedia {
        upload_id: upload.id,
        conversation_id: upload.conversation_id,
        kind: MediaKind::Image,
        bytes: upload.attachment.size,
        thumb_bytes: Some(thumb.size),
    })
}

pub async fn commit(api: &Api, store: &Store, id: Uuid, now: Millis) -> ChatResult<CommitAnswer> {
    let upload = store.upload(id)?.ok_or(ChatError::NotFound)?;
    let media_id = upload.media_id.ok_or(ChatError::Invalid)?;
    match api.commit_media(media_id).await {
        Ok(answer) => {
            store.upload_committed(id, now)?;
            Ok(answer)
        }
        Err(ChatError::MediaMissing { ids }) => {
            let only_thumb = !ids.is_empty() && !ids.contains(&media_id);
            let object = if only_thumb {
                UploadObject::Thumb
            } else {
                UploadObject::Image
            };
            store.upload_redo(id, object, now)?;
            Err(ChatError::MediaMissing { ids })
        }
        Err(ChatError::NotFound) => {
            store.upload_restart(id, now)?;
            Err(ChatError::NotFound)
        }
        Err(error) => Err(error),
    }
}

pub async fn urls(
    api: &Api,
    conversation_id: Option<ConvId>,
    ids: &[Uuid],
) -> ChatResult<UrlsAnswer> {
    let mut items = Vec::with_capacity(ids.len());
    let mut expires_in_s = None;
    for chunk in ids.chunks(URLS_MAX) {
        let request = UrlsRequest {
            conversation_id,
            ids: chunk.to_vec(),
        };
        let part = api.media_urls(&request).await?;
        expires_in_s =
            Some(expires_in_s.map_or(part.expires_in_s, |s: u32| s.min(part.expires_in_s)));
        items.extend(part.items);
    }
    Ok(UrlsAnswer {
        expires_in_s: expires_in_s.unwrap_or_default(),
        items,
    })
}
