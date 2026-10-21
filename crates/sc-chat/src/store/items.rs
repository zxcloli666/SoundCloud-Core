use rusqlite::{Connection, OptionalExtension, Params, Row, params};

use super::codec::{class_at, from_json, to_json, uuid_at, uuid_opt_at};
use super::seal::{self, Sealed};
use super::{StoreKeys, StoredContent, StoredMessage};
use crate::contracts::{ChatResult, ConvId, Seq};

const COLUMNS: &str = "id, conversation_id, seq, client_id, sender, sender_device, class, epoch, target_seq, ts, expires_at, local_only, content, nonce, body";

pub struct Item {
    pub id: i64,
    pub message: StoredMessage,
}

struct Raw {
    id: i64,
    head: StoredMessage,
    kind: String,
    nonce: Option<Vec<u8>>,
    body: Option<Vec<u8>>,
}

pub fn load<P: Params>(
    conn: &Connection,
    keys: &StoreKeys,
    filter: &str,
    params: P,
) -> ChatResult<Vec<Item>> {
    let mut stmt =
        conn.prepare_cached(&format!("SELECT {COLUMNS} FROM messages WHERE {filter}"))?;
    let raws = stmt
        .query_map(params, raw)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(raws.into_iter().map(|raw| open(raw, keys)).collect())
}

pub fn by_seq(
    conn: &Connection,
    keys: &StoreKeys,
    conversation_id: ConvId,
    seq: Seq,
) -> ChatResult<Option<Item>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {COLUMNS} FROM messages WHERE conversation_id = ?1 AND seq = ?2"
    ))?;
    let found = stmt
        .query_row(params![conversation_id.as_bytes(), seq], raw)
        .optional()?;
    Ok(found.map(|raw| open(raw, keys)))
}

pub fn targeting(
    conn: &Connection,
    keys: &StoreKeys,
    conversation_id: ConvId,
    from: Seq,
    to: Seq,
) -> ChatResult<Vec<Item>> {
    Ok(sorted(load(
        conn,
        keys,
        "conversation_id = ?1 AND target_seq BETWEEN ?2 AND ?3 AND class IN ('edit', 'react', 'pin')",
        params![conversation_id.as_bytes(), from, to],
    )?))
}

pub fn edits(
    conn: &Connection,
    keys: &StoreKeys,
    conversation_id: ConvId,
    target: Seq,
) -> ChatResult<Vec<Item>> {
    Ok(sorted(load(
        conn,
        keys,
        "conversation_id = ?1 AND target_seq = ?2 AND class = 'edit'",
        params![conversation_id.as_bytes(), target],
    )?))
}

fn sorted(mut items: Vec<Item>) -> Vec<Item> {
    items.sort_by_key(|item| item.message.seq);
    items
}

fn raw(row: &Row<'_>) -> rusqlite::Result<Raw> {
    Ok(Raw {
        id: row.get(0)?,
        head: StoredMessage {
            conversation_id: uuid_at(row, 1)?,
            seq: row.get(2)?,
            client_id: uuid_at(row, 3)?,
            sender: row.get(4)?,
            sender_device: uuid_opt_at(row, 5)?,
            class: class_at(row, 6)?,
            epoch: row.get(7)?,
            target_seq: row.get(8)?,
            ts: row.get(9)?,
            expires_at: row.get(10)?,
            local_only: row.get::<_, i64>(11)? != 0,
            content: StoredContent::Deleted,
        },
        kind: row.get(12)?,
        nonce: row.get(13)?,
        body: row.get(14)?,
    })
}

fn open(raw: Raw, keys: &StoreKeys) -> Item {
    let mut message = raw.head;
    message.content = match (raw.nonce, raw.body) {
        (Some(nonce), Some(body)) => {
            let aad = seal::message_aad(message.conversation_id, message.seq);
            seal::open(&keys.body, &aad, &nonce, &body)
                .ok()
                .and_then(|plain| content(&raw.kind, plain.to_vec()))
                .unwrap_or(StoredContent::Deleted)
        }
        _ => StoredContent::Deleted,
    };
    Item {
        id: raw.id,
        message,
    }
}

fn content(kind: &str, plain: Vec<u8>) -> Option<StoredContent> {
    match kind {
        "open" => Some(StoredContent::Open(plain)),
        "sealed" => Some(StoredContent::Sealed(plain)),
        "system" => from_json(&plain).ok().map(StoredContent::System),
        _ => None,
    }
}

pub fn seal_content(
    keys: &StoreKeys,
    conversation_id: ConvId,
    seq: Seq,
    content: &StoredContent,
) -> ChatResult<(&'static str, Option<Sealed>)> {
    let aad = seal::message_aad(conversation_id, seq);
    let sealed = |plain: &[u8]| seal::seal(&keys.body, &aad, plain).map(Some);
    Ok(match content {
        StoredContent::Open(plain) => ("open", sealed(plain)?),
        StoredContent::Sealed(body) => ("sealed", sealed(body)?),
        StoredContent::System(event) => ("system", sealed(&to_json(event)?)?),
        StoredContent::Deleted => ("deleted", None),
    })
}
