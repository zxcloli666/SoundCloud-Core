use sc_chat_wire::proto::Attachment;

use super::Store;
use crate::contracts::{
    BackdropChange, ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta, MediaKind,
};

impl Store {
    pub fn backdrop_meta(
        &self,
        room: ConvId,
        change: &BackdropChange,
    ) -> ChatResult<ConversationMeta> {
        if self.not_a_room(room)? {
            return Err(ChatError::Invalid);
        }
        let avatar = match change {
            BackdropChange::Clear => None,
            BackdropChange::Set { upload_id } => {
                Some(self.committed_upload(room, *upload_id, MediaKind::Backdrop)?)
            }
        };
        Ok(ConversationMeta {
            title: String::new(),
            description: String::new(),
            avatar,
        })
    }

    pub fn room_backdrop(&self, room: ConvId) -> ChatResult<Option<Attachment>> {
        if self.not_a_room(room)? {
            return Ok(None);
        }
        Ok(self.group_meta(room)?.and_then(|meta| meta.avatar))
    }

    pub fn put_room_meta(&self, room: ConvId, meta: Option<&ConversationMeta>) -> ChatResult<()> {
        match meta {
            Some(meta) => self.put_group_meta(room, meta),
            None => self.drop_group_meta(room),
        }
    }

    fn not_a_room(&self, id: ConvId) -> ChatResult<bool> {
        Ok(!self
            .conversation(id)?
            .is_some_and(|summary| summary.kind == ConversationKind::Room))
    }
}
