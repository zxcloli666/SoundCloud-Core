use rusqlite::{Connection, OptionalExtension, params};
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::Attachment;
use uuid::Uuid;

use super::items::Item;
use super::{Store, StoreKeys, StoredContent, content, group_meta, seal};
use crate::contracts::{ChatError, ChatResult, ConvId};
use crate::envelope::Content;

impl Store {
    pub fn attachment(
        &self,
        conversation_id: ConvId,
        media_id: Uuid,
    ) -> ChatResult<Option<Attachment>> {
        let conn = self.conn()?;
        match in_messages(&conn, &self.keys, conversation_id, media_id)? {
            Some(found) => Ok(Some(found)),
            None => group_meta::avatar(&conn, &self.keys, conversation_id, media_id),
        }
    }
}

pub fn of(item: &Item) -> Vec<Attachment> {
    match &item.message.content {
        StoredContent::Open(bytes) => match content::decode(bytes) {
            Content::Post(post) => post.attachments,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

pub fn replace(
    tx: &Connection,
    keys: &StoreKeys,
    message_id: i64,
    conv: ConvId,
    attachments: &[Attachment],
) -> ChatResult<()> {
    tx.prepare_cached("DELETE FROM message_media WHERE message_id = ?1")?
        .execute([message_id])?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO message_media (message_id, position, conversation_id, media_id, nonce, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (position, attachment) in attachments.iter().enumerate() {
        let Ok(media_id) = Uuid::parse_str(&attachment.media_id) else {
            continue;
        };
        let position = position as u32;
        let sealed = seal::seal(
            &keys.body,
            &aad(message_id, position),
            &attachment.encode_to_vec(),
        )?;
        insert.execute(params![
            message_id,
            position,
            conv.as_bytes(),
            media_id.as_bytes(),
            sealed.nonce,
            sealed.body
        ])?;
    }
    Ok(())
}

fn in_messages(
    conn: &Connection,
    keys: &StoreKeys,
    conv: ConvId,
    media_id: Uuid,
) -> ChatResult<Option<Attachment>> {
    let found: Option<(i64, u32, Vec<u8>, Vec<u8>)> = conn
        .prepare_cached(
            "SELECT message_id, position, nonce, body FROM message_media WHERE conversation_id = ?1 AND media_id = ?2 LIMIT 1",
        )?
        .query_row(params![conv.as_bytes(), media_id.as_bytes()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .optional()?;
    let Some((message_id, position, nonce, body)) = found else {
        return Ok(None);
    };
    let plain = seal::open(&keys.body, &aad(message_id, position), &nonce, &body)?;
    Attachment::decode(plain.as_slice())
        .map(Some)
        .map_err(|_| ChatError::Store)
}

fn aad(message_id: i64, position: u32) -> Vec<u8> {
    seal::tagged_aad(
        "message-media",
        &[&message_id.to_be_bytes(), &position.to_be_bytes()],
    )
}
