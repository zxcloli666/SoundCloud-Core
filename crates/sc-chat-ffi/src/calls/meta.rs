use sc_chat::envelope::Content;
use sc_chat::{
    AvatarChange, BackdropChange, ChatResult, ConvId, ConversationPatch, Message, RoomBackdrop,
    RoomSettingsPatch,
};

use crate::runtime::Chat;
use crate::types::MediaInfo;

impl Chat {
    pub async fn set_group_avatar(
        &self,
        conversation_id: ConvId,
        change: AvatarChange,
    ) -> ChatResult<Message> {
        let patch = ConversationPatch {
            avatar: Some(change),
            ..ConversationPatch::default()
        };
        let meta = self
            .blocking(move |core| core.store.next_meta(conversation_id, &patch))
            .await?;
        self.queue(conversation_id, None, Content::Meta(meta)).await
    }

    pub async fn room_backdrop(&self, conversation_id: ConvId) -> ChatResult<Option<MediaInfo>> {
        self.blocking(move |core| {
            Ok(core
                .store
                .room_backdrop(conversation_id)?
                .as_ref()
                .and_then(MediaInfo::of))
        })
        .await
    }

    pub async fn set_room_backdrop(
        &self,
        room_code: String,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> ChatResult<Message> {
        let meta = self
            .blocking(move |core| core.store.backdrop_meta(conversation_id, &change))
            .await?;
        let backdrop = match &meta.avatar {
            Some(image) => Some(RoomBackdrop::Image {
                media_id: sc_chat::media::image_id(image)?,
            }),
            None => None,
        };
        self.room_settings(&room_code, backdrop).await?;
        self.queue(conversation_id, None, Content::Meta(meta)).await
    }

    pub async fn set_room_preset(
        &self,
        room_code: String,
        conversation_id: ConvId,
        preset_id: String,
    ) -> ChatResult<Option<Message>> {
        let (cleared, known_bare) = self
            .blocking(move |core| {
                let cleared = core
                    .store
                    .backdrop_meta(conversation_id, &BackdropChange::Clear)?;
                let known_bare = core
                    .store
                    .group_meta(conversation_id)?
                    .is_some_and(|meta| meta.avatar.is_none());
                Ok((cleared, known_bare))
            })
            .await?;
        self.room_settings(&room_code, Some(RoomBackdrop::Preset { id: preset_id }))
            .await?;
        if known_bare {
            return Ok(None);
        }
        self.queue(conversation_id, None, Content::Meta(cleared))
            .await
            .map(Some)
    }

    async fn room_settings(
        &self,
        room_code: &str,
        backdrop: Option<RoomBackdrop>,
    ) -> ChatResult<()> {
        self.ready()?;
        let patch = RoomSettingsPatch {
            backdrop: Some(backdrop),
            ..RoomSettingsPatch::default()
        };
        self.api.room_settings(room_code, &patch).await.map(drop)
    }
}
