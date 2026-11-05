use sc_chat::envelope::Content;
use sc_chat::media::BackdropStep;
use sc_chat::{AvatarChange, BackdropChange, ChatResult, ConvId, ConversationPatch, Message};

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
        self.queue(conversation_id, None, Content::Meta(meta)).await
    }

    pub async fn set_room_backdrop(
        &self,
        conversation_id: ConvId,
        change: BackdropChange,
    ) -> ChatResult<Message> {
        let step = self
            .blocking(move |core| BackdropStep::set(&core.store, conversation_id, &change))
            .await?;
        let meta = step.send(&self.api).await?;
        self.queue(conversation_id, None, Content::Meta(meta)).await
    }

    pub async fn set_room_preset(
        &self,
        conversation_id: ConvId,
        preset_id: String,
    ) -> ChatResult<Option<Message>> {
        let step = self
            .blocking(move |core| BackdropStep::preset(&core.store, conversation_id, preset_id))
            .await?;
        match step.send(&self.api).await? {
            Some(cleared) => self
                .queue(conversation_id, None, Content::Meta(cleared))
                .await
                .map(Some),
            None => Ok(None),
        }
    }
}
