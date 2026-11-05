use uuid::Uuid;

use super::URLS_MAX;
use crate::api::Api;
use crate::api::wire::{
    CommitAnswer, MediaSelection, PresignedPut, ReserveMedia, UrlsAnswer, UrlsRequest,
};
use crate::contracts::{ChatError, ChatResult, ConvId, MediaKind, Millis};
use crate::store::{MediaUpload, Reserved, Store, UploadObject, UploadState};

pub const PRESIGN_MARGIN_MS: Millis = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutTargets {
    pub expires_at: Millis,
    pub image: PresignedPut,
    pub thumb: Option<PresignedPut>,
}

pub async fn reserve(api: &Api, store: &Store, id: Uuid, now: Millis) -> ChatResult<PutTargets> {
    let upload = store.upload(id)?.ok_or(ChatError::NotFound)?;
    let answer = api.reserve_media(&reserve_request(&upload)?).await?;
    if (upload.kind == MediaKind::Image) != answer.thumb.is_some() {
        return Err(ChatError::Protocol);
    }
    let expires_at = now.saturating_add(i64::from(answer.expires_in_s) * 1_000 - PRESIGN_MARGIN_MS);
    let reserved = Reserved {
        media_id: answer.id,
        thumb_media_id: answer.thumb.as_ref().map(|thumb| thumb.id),
        presign_expires_at: expires_at,
    };
    store.upload_reserved(id, &reserved, now)?;
    Ok(PutTargets {
        expires_at,
        image: answer.put,
        thumb: answer.thumb.map(|thumb| thumb.put),
    })
}

pub fn reserve_request(upload: &MediaUpload) -> ChatResult<ReserveMedia> {
    let thumb_bytes = match upload.kind {
        MediaKind::Image => Some(
            upload
                .attachment
                .thumb
                .as_ref()
                .ok_or(ChatError::Invalid)?
                .size,
        ),
        MediaKind::Avatar | MediaKind::Backdrop => None,
    };
    Ok(ReserveMedia {
        upload_id: upload.id,
        conversation_id: upload.conversation_id,
        kind: upload.kind,
        bytes: upload.attachment.size,
        thumb_bytes,
    })
}

pub async fn commit(api: &Api, store: &Store, id: Uuid, now: Millis) -> ChatResult<CommitAnswer> {
    let upload = store.upload(id)?.ok_or(ChatError::NotFound)?;
    let media_id = upload.media_id.ok_or(ChatError::Invalid)?;
    match api.commit_media(media_id).await {
        Ok(answer) => match store.upload_committed(id, now) {
            Err(ChatError::NotFound) => {
                if upload.state != UploadState::Committed && !store.was_committed(id)? {
                    let _ = api.delete_media(&MediaSelection::ids(vec![media_id])).await;
                }
                Err(ChatError::NotFound)
            }
            committed => committed.map(|()| answer),
        },
        Err(ChatError::MediaMissing { ids }) => {
            let only_thumb =
                upload.kind == MediaKind::Image && !ids.is_empty() && !ids.contains(&media_id);
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
