use crate::api::Api;
use crate::contracts::{
    BackdropChange, ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta,
    ConversationSummary, RoomBackdrop, RoomSettingsPatch,
};
use crate::store::{Store, backdrop_slot};

use super::image_id;

pub struct BackdropStep {
    target: Target,
    backdrop: Option<RoomBackdrop>,
    meta: Option<ConversationMeta>,
}

enum Target {
    Room { code: String },
    Conversation { id: ConvId },
}

impl BackdropStep {
    pub fn set(store: &Store, id: ConvId, change: &BackdropChange) -> ChatResult<Self> {
        let summary = summary(store, id)?;
        let meta = store.backdrop_meta(id, change)?;
        let image = backdrop_slot(summary.kind, meta.clone());
        let (backdrop, meta) = match image {
            Some(image) => {
                let backdrop = RoomBackdrop::Image {
                    media_id: image_id(&image)?,
                };
                (Some(backdrop), Some(meta))
            }
            None => (None, drops_key(store, &summary)?.then_some(meta)),
        };
        Ok(Self {
            target: target(&summary)?,
            backdrop,
            meta,
        })
    }

    pub fn preset(store: &Store, id: ConvId, preset_id: String) -> ChatResult<Self> {
        let summary = summary(store, id)?;
        let cleared = store.backdrop_meta(id, &BackdropChange::Clear)?;
        Ok(Self {
            target: target(&summary)?,
            backdrop: Some(RoomBackdrop::Preset { id: preset_id }),
            meta: drops_key(store, &summary)?.then_some(cleared),
        })
    }

    pub async fn send(self, api: &Api) -> ChatResult<Option<ConversationMeta>> {
        match &self.target {
            Target::Room { code } => {
                let patch = RoomSettingsPatch {
                    backdrop: Some(self.backdrop),
                    ..RoomSettingsPatch::default()
                };
                api.room_settings(code, &patch).await?;
            }
            Target::Conversation { id } => {
                api.set_backdrop(*id, self.backdrop.as_ref()).await?;
            }
        }
        Ok(self.meta)
    }
}

fn summary(store: &Store, id: ConvId) -> ChatResult<ConversationSummary> {
    store.conversation(id)?.ok_or(ChatError::Invalid)
}

fn target(summary: &ConversationSummary) -> ChatResult<Target> {
    Ok(match summary.kind {
        ConversationKind::Room => Target::Room {
            code: summary.room_code.clone().ok_or(ChatError::NotFound)?,
        },
        ConversationKind::Unknown => return Err(ChatError::Invalid),
        _ => Target::Conversation { id: summary.id },
    })
}

fn drops_key(store: &Store, summary: &ConversationSummary) -> ChatResult<bool> {
    Ok(match summary.kind {
        ConversationKind::Room => {
            let known = store.group_meta(summary.id)?;
            known.is_none_or(|meta| meta.avatar.is_some())
        }
        ConversationKind::Group => store.backdrop_image(summary.id)?.is_some(),
        _ => false,
    })
}
