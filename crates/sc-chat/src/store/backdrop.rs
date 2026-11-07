use sc_chat_wire::proto::Attachment;

use super::Store;
use super::codec::uuid_at;
use crate::contracts::{
    BackdropChange, ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta, MediaKind,
};

impl Store {
    pub fn backdrop_meta(
        &self,
        id: ConvId,
        change: &BackdropChange,
    ) -> ChatResult<ConversationMeta> {
        let summary = self.conversation(id)?.ok_or(ChatError::Invalid)?;
        let image = match change {
            BackdropChange::Clear => None,
            BackdropChange::Set { upload_id } => {
                Some(self.committed_upload(id, *upload_id, MediaKind::Backdrop)?)
            }
        };
        Ok(match summary.kind {
            ConversationKind::Room => ConversationMeta {
                avatar: image,
                ..ConversationMeta::default()
            },
            ConversationKind::Group => ConversationMeta {
                backdrop: image,
                ..self.known_meta(id, summary.title)?
            },
            ConversationKind::Direct | ConversationKind::Saved | ConversationKind::Public => {
                ConversationMeta {
                    backdrop: image,
                    ..ConversationMeta::default()
                }
            }
            ConversationKind::Unknown => return Err(ChatError::Invalid),
        })
    }

    pub fn backdrop_image(&self, id: ConvId) -> ChatResult<Option<Attachment>> {
        let Some(summary) = self.conversation(id)? else {
            return Ok(None);
        };
        let Some(meta) = self.group_meta(id)? else {
            return Ok(None);
        };
        Ok(backdrop_slot(summary.kind, meta))
    }

    pub fn room_backdrop(&self, room: ConvId) -> ChatResult<Option<Attachment>> {
        if !self.is_room(room)? {
            return Ok(None);
        }
        self.backdrop_image(room)
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

    fn is_room(&self, id: ConvId) -> ChatResult<bool> {
        Ok(self
            .conversation(id)?
            .is_some_and(|summary| summary.kind == ConversationKind::Room))
    }
}

pub fn backdrop_slot(kind: ConversationKind, meta: ConversationMeta) -> Option<Attachment> {
    match kind {
        ConversationKind::Room => meta.avatar,
        _ => meta.backdrop,
    }
}
