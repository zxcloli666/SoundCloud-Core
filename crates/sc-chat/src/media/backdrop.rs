use crate::api::Api;
use crate::contracts::{
    BackdropChange, ChatError, ChatResult, ConvId, ConversationMeta, RoomBackdrop,
    RoomSettingsPatch,
};
use crate::store::Store;

use super::image_id;

pub struct BackdropStep<M> {
    room_code: String,
    backdrop: Option<RoomBackdrop>,
    meta: M,
}

impl BackdropStep<ConversationMeta> {
    pub fn set(store: &Store, room: ConvId, change: &BackdropChange) -> ChatResult<Self> {
        let meta = store.backdrop_meta(room, change)?;
        let backdrop = match &meta.avatar {
            Some(image) => Some(RoomBackdrop::Image {
                media_id: image_id(image)?,
            }),
            None => None,
        };
        Ok(Self {
            room_code: room_code(store, room)?,
            backdrop,
            meta,
        })
    }
}

impl BackdropStep<Option<ConversationMeta>> {
    pub fn preset(store: &Store, room: ConvId, preset_id: String) -> ChatResult<Self> {
        let cleared = store.backdrop_meta(room, &BackdropChange::Clear)?;
        let bare = store
            .group_meta(room)?
            .is_some_and(|meta| meta.avatar.is_none());
        Ok(Self {
            room_code: room_code(store, room)?,
            backdrop: Some(RoomBackdrop::Preset { id: preset_id }),
            meta: (!bare).then_some(cleared),
        })
    }
}

impl<M> BackdropStep<M> {
    pub async fn send(self, api: &Api) -> ChatResult<M> {
        let patch = RoomSettingsPatch {
            backdrop: Some(self.backdrop),
            ..RoomSettingsPatch::default()
        };
        api.room_settings(&self.room_code, &patch).await?;
        Ok(self.meta)
    }
}

fn room_code(store: &Store, room: ConvId) -> ChatResult<String> {
    store
        .conversation(room)?
        .and_then(|summary| summary.room_code)
        .ok_or(ChatError::NotFound)
}
