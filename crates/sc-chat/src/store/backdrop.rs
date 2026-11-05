use sc_chat_wire::proto::Attachment;

use super::Store;
use super::codec::uuid_at;
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

    pub fn quiet_rooms(&self, limit: usize) -> ChatResult<Vec<ConvId>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM conversations c WHERE kind = 'room' AND state = 'active' AND NOT EXISTS (SELECT 1 FROM group_meta g WHERE g.conversation_id = c.id) ORDER BY last_at DESC, id LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| uuid_at(row, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn not_a_room(&self, id: ConvId) -> ChatResult<bool> {
        Ok(!self
            .conversation(id)?
            .is_some_and(|summary| summary.kind == ConversationKind::Room))
    }
}
