use rusqlite::{Connection, OptionalExtension, params};
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::{Attachment, Envelope};
use uuid::Uuid;

use super::{Store, StoreKeys, seal};
use crate::contracts::{
    AvatarChange, ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta,
    ConversationPatch, MediaKind, UploadState,
};
use crate::envelope::{self, Content};

impl Store {
    pub fn group_meta(&self, id: ConvId) -> ChatResult<Option<ConversationMeta>> {
        let conn = self.conn()?;
        read(&conn, &self.keys, id, None)
    }

    pub fn put_group_meta(&self, id: ConvId, meta: &ConversationMeta) -> ChatResult<()> {
        let envelope = envelope::encode(&Content::Meta(meta.clone()))?;
        let sealed = seal::seal(&self.keys.body, &aad(id), &envelope.encode_to_vec())?;
        let avatar_id = meta
            .avatar
            .as_ref()
            .and_then(|avatar| Uuid::parse_str(&avatar.media_id).ok());
        self.conn()?.execute(
            "INSERT INTO group_meta (conversation_id, avatar_id, nonce, body) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (conversation_id) DO UPDATE SET avatar_id = excluded.avatar_id, nonce = excluded.nonce, body = excluded.body",
            params![
                id.as_bytes(),
                avatar_id.map(|id| *id.as_bytes()),
                sealed.nonce,
                sealed.body
            ],
        )?;
        Ok(())
    }

    pub fn next_meta(&self, id: ConvId, patch: &ConversationPatch) -> ChatResult<ConversationMeta> {
        let summary = self
            .conversation(id)?
            .filter(|summary| summary.kind == ConversationKind::Group)
            .ok_or(ChatError::Invalid)?;
        let mut next = match self.group_meta(id)? {
            Some(meta) => meta,
            None => ConversationMeta {
                title: summary.title.unwrap_or_default(),
                ..ConversationMeta::default()
            },
        };
        if let Some(meta) = &patch.meta {
            if let Some(title) = &meta.title {
                next.title.clone_from(title);
            }
            if let Some(description) = &meta.description {
                next.description.clone_from(description);
            }
        }
        match patch.avatar {
            None => {}
            Some(AvatarChange::Clear) => next.avatar = None,
            Some(AvatarChange::Set { upload_id }) => {
                next.avatar = Some(self.committed_upload(id, upload_id, MediaKind::Avatar)?);
            }
        }
        Ok(next)
    }

    pub(super) fn drop_group_meta(&self, id: ConvId) -> ChatResult<()> {
        self.conn()?.execute(
            "DELETE FROM group_meta WHERE conversation_id = ?1",
            [id.as_bytes()],
        )?;
        Ok(())
    }

    pub(super) fn committed_upload(
        &self,
        id: ConvId,
        upload_id: Uuid,
        kind: MediaKind,
    ) -> ChatResult<Attachment> {
        let upload = self.upload(upload_id)?.ok_or(ChatError::NotFound)?;
        let ready = upload.kind == kind
            && upload.state == UploadState::Committed
            && upload.conversation_id == id;
        match ready {
            true => Ok(upload.attachment),
            false => Err(ChatError::Invalid),
        }
    }
}

pub fn avatar(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    media_id: Uuid,
) -> ChatResult<Option<Attachment>> {
    Ok(read(conn, keys, id, Some(media_id))?.and_then(|meta| meta.avatar))
}

fn read(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    avatar_id: Option<Uuid>,
) -> ChatResult<Option<ConversationMeta>> {
    let found: Option<(Vec<u8>, Vec<u8>)> = conn
        .prepare_cached(
            "SELECT nonce, body FROM group_meta WHERE conversation_id = ?1 AND (?2 IS NULL OR avatar_id = ?2)",
        )?
        .query_row(
            params![id.as_bytes(), avatar_id.map(|id| *id.as_bytes())],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((nonce, body)) = found else {
        return Ok(None);
    };
    let plain = seal::open(&keys.body, &aad(id), &nonce, &body)?;
    let envelope = Envelope::decode(plain.as_slice()).map_err(|_| ChatError::Store)?;
    match envelope::decode(&envelope) {
        Ok(Content::Meta(meta)) => Ok(Some(meta)),
        _ => Err(ChatError::Store),
    }
}

fn aad(id: ConvId) -> Vec<u8> {
    seal::tagged_aad("group-meta", &[id.as_bytes()])
}
