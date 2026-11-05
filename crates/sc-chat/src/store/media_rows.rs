use std::fmt;

use rusqlite::{Connection, Row};
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::Attachment;
use uuid::Uuid;

use super::codec::Label;
use super::{StoreKeys, seal};
use crate::contracts::{
    ChatError, ChatResult, ConvId, MediaKind, Millis, UploadObject, UploadState,
};

#[derive(Clone, PartialEq)]
pub struct MediaUpload {
    pub id: Uuid,
    pub album: Uuid,
    pub position: u32,
    pub conversation_id: ConvId,
    pub kind: MediaKind,
    pub image_ref: String,
    pub thumb_ref: Option<String>,
    pub attachment: Attachment,
    pub state: UploadState,
    pub object: UploadObject,
    pub media_id: Option<Uuid>,
    pub stage_id: Option<String>,
    pub parts_sent: u32,
    pub presign_expires_at: Option<Millis>,
    pub attempts: u32,
    pub created_at: Millis,
    pub updated_at: Millis,
}

impl MediaUpload {
    pub fn presign_expired(&self, now: Millis) -> bool {
        self.presign_expires_at.is_none_or(|at| at <= now)
    }
}

#[derive(Clone)]
pub struct NewUpload {
    pub album: Uuid,
    pub position: u32,
    pub conversation_id: ConvId,
    pub kind: MediaKind,
    pub image_ref: String,
    pub thumb_ref: Option<String>,
    pub attachment: Attachment,
}

impl fmt::Debug for MediaUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MediaUpload")
            .field("id", &self.id)
            .field("album", &self.album)
            .field("position", &self.position)
            .field("conversation_id", &self.conversation_id)
            .field("kind", &self.kind)
            .field("image_ref", &self.image_ref)
            .field("thumb_ref", &self.thumb_ref)
            .field("attachment", &Unkeyed(&self.attachment))
            .field("state", &self.state)
            .field("object", &self.object)
            .field("media_id", &self.media_id)
            .field("stage_id", &self.stage_id)
            .field("parts_sent", &self.parts_sent)
            .field("presign_expires_at", &self.presign_expires_at)
            .field("attempts", &self.attempts)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl fmt::Debug for NewUpload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewUpload")
            .field("album", &self.album)
            .field("position", &self.position)
            .field("conversation_id", &self.conversation_id)
            .field("kind", &self.kind)
            .field("image_ref", &self.image_ref)
            .field("thumb_ref", &self.thumb_ref)
            .field("attachment", &Unkeyed(&self.attachment))
            .finish()
    }
}

struct Unkeyed<'a>(&'a Attachment);

impl fmt::Debug for Unkeyed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let thumb = self.0.thumb.as_ref();
        f.debug_struct("Attachment")
            .field("media_id", &self.0.media_id)
            .field("size", &self.0.size)
            .field("w", &self.0.w)
            .field("h", &self.0.h)
            .field("thumb_media_id", &thumb.map(|thumb| &thumb.media_id))
            .field("thumb_size", &thumb.map(|thumb| thumb.size))
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct Reserved {
    pub media_id: Uuid,
    pub thumb_media_id: Option<Uuid>,
    pub presign_expires_at: Millis,
}

const COLUMNS: &str = "id, album, position, conversation_id, kind, image_ref, thumb_ref, state, object, media_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment";

pub fn load(
    conn: &Connection,
    keys: &StoreKeys,
    filter: &str,
    params: impl rusqlite::Params,
) -> ChatResult<Vec<MediaUpload>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM media_uploads WHERE {filter}"
    ))?;
    let rows = statement
        .query_map(params, raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|row| row.open(keys)).collect()
}

pub fn aad(id: Uuid) -> Vec<u8> {
    seal::tagged_aad("media-upload", &[id.as_bytes()])
}

struct Raw {
    upload: MediaUpload,
    nonce: Vec<u8>,
    sealed: Vec<u8>,
}

impl Raw {
    fn open(mut self, keys: &StoreKeys) -> ChatResult<MediaUpload> {
        let plain = seal::open(&keys.body, &aad(self.upload.id), &self.nonce, &self.sealed)?;
        self.upload.attachment =
            Attachment::decode(plain.as_slice()).map_err(|_| ChatError::Store)?;
        Ok(self.upload)
    }
}

fn raw(row: &Row) -> rusqlite::Result<Raw> {
    let uuid = |bytes: Vec<u8>| -> rusqlite::Result<Uuid> {
        Uuid::from_slice(&bytes).map_err(|_| rusqlite::Error::InvalidQuery)
    };
    Ok(Raw {
        upload: MediaUpload {
            id: uuid(row.get(0)?)?,
            album: uuid(row.get(1)?)?,
            position: row.get(2)?,
            conversation_id: uuid(row.get(3)?)?,
            kind: row.get::<_, Label<_>>(4)?.0,
            image_ref: row.get(5)?,
            thumb_ref: row.get(6)?,
            attachment: Attachment::default(),
            state: row.get::<_, Label<_>>(7)?.0,
            object: row.get::<_, Label<_>>(8)?.0,
            media_id: row.get::<_, Option<Vec<u8>>>(9)?.map(uuid).transpose()?,
            stage_id: row.get(10)?,
            parts_sent: row.get(11)?,
            presign_expires_at: row.get(12)?,
            attempts: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        },
        nonce: row.get(16)?,
        sealed: row.get(17)?,
    })
}

#[cfg(test)]
#[path = "media_rows_tests.rs"]
mod tests;
