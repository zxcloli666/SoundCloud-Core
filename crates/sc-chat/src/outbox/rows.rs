use rusqlite::{Connection, OptionalExtension, Params, params};

use super::OutboxItem;
use crate::contracts::{ChatResult, ClientId, ConvId, Millis, Seq};
use crate::store::{StoreKeys, codec, seal};

pub fn load<P: Params>(
    conn: &Connection,
    keys: &StoreKeys,
    filter: &str,
    params: P,
) -> ChatResult<Vec<(OutboxItem, bool)>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT client_id, conversation_id, class, target_seq, created_at, attempts, next_at, failed, nonce, body FROM outbox WHERE {filter}"
    ))?;
    let rows = stmt
        .query_map(params, |row| {
            Ok((
                codec::uuid_at(row, 0)?,
                codec::uuid_at(row, 1)?,
                codec::class_at(row, 2)?,
                row.get::<_, Option<Seq>>(3)?,
                row.get::<_, Millis>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, Millis>(6)?,
                row.get::<_, bool>(7)?,
                row.get::<_, Vec<u8>>(8)?,
                row.get::<_, Vec<u8>>(9)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::with_capacity(rows.len());
    for (
        client_id,
        conversation_id,
        class,
        target_seq,
        created_at,
        attempts,
        next_at,
        failed,
        nonce,
        body,
    ) in rows
    {
        let envelope = seal::open(&keys.body, &aad(client_id), &nonce, &body)?;
        items.push((
            OutboxItem {
                client_id,
                conversation_id,
                class,
                target_seq,
                envelope: envelope.to_vec(),
                created_at,
                attempts,
                next_at,
            },
            failed,
        ));
    }
    Ok(items)
}

pub fn aad(client_id: ClientId) -> Vec<u8> {
    seal::tagged_aad("outbox", &[client_id.as_bytes()])
}

pub fn has_message(conn: &Connection, conv: ConvId, seq: Seq) -> ChatResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM messages WHERE conversation_id = ?1 AND seq = ?2)",
        params![conv.as_bytes(), seq],
        |row| row.get(0),
    )?)
}

pub fn key_epoch(conn: &Connection, conv: ConvId) -> ChatResult<u32> {
    Ok(conn
        .query_row(
            "SELECT key_epoch FROM conversations WHERE id = ?1",
            [conv.as_bytes()],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default())
}
