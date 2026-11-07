use sc_chat::envelope::Content;
use sc_chat::media::BackdropStep;
use sc_chat::{
    AvatarChange, BackdropChange, ChatError, ChatResult, ConvId, ConversationKind,
    ConversationMeta, ConversationPatch, Message,
};

use crate::runtime::Chat;

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
        self.queue(conversation_id, None, Content::Meta(Box::new(meta)))
            .await
    }

    pub async fn set_backdrop(
        &self,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> ChatResult<Option<Message>> {
        let step = self
            .blocking(move |core| BackdropStep::set(&core.store, conversation_id, &change))
            .await?;
        let meta = step.send(&self.api).await?;
        self.queue_meta(conversation_id, meta).await
    }

    pub async fn set_backdrop_preset(
        &self,
        conversation_id: ConvId,
        preset_id: String,
    ) -> ChatResult<Option<Message>> {
        let step = self
            .blocking(move |core| BackdropStep::preset(&core.store, conversation_id, preset_id))
            .await?;
        let meta = step.send(&self.api).await?;
        self.queue_meta(conversation_id, meta).await
    }

    pub async fn set_room_backdrop(
        &self,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> ChatResult<Message> {
        self.room_only(conversation_id).await?;
        self.set_backdrop(conversation_id, change)
            .await?
            .ok_or(ChatError::Internal)
    }

    pub async fn set_room_preset(
        &self,
        conversation_id: ConvId,
        preset_id: String,
    ) -> ChatResult<Option<Message>> {
        self.room_only(conversation_id).await?;
        self.set_backdrop_preset(conversation_id, preset_id).await
    }

    async fn room_only(&self, conversation_id: ConvId) -> ChatResult<()> {
        let room = self
            .blocking(move |core| core.store.conversation(conversation_id))
            .await?
            .is_some_and(|summary| summary.kind == ConversationKind::Room);
        match room {
            true => Ok(()),
            false => Err(ChatError::Invalid),
        }
    }

    async fn queue_meta(
        &self,
        conversation_id: ConvId,
        meta: Option<ConversationMeta>,
    ) -> ChatResult<Option<Message>> {
        match meta {
            Some(meta) => self
                .queue(conversation_id, None, Content::Meta(Box::new(meta)))
                .await
                .map(Some),
            None => Ok(None),
        }
    }
}
