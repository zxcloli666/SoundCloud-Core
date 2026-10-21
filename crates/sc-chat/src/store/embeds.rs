use rusqlite::{Connection, params};

use super::codec::{from_json, label, to_json};
use super::seal;
use super::{Store, StoreKeys};
use crate::contracts::{ChatResult, ConvId, Embed, EmbedHit, EmbedPage, EmbedsQuery};

const PAGE_DEFAULT: u32 = 30;
const PAGE_MAX: u32 = 100;

impl Store {
    pub fn embeds(&self, id: ConvId, query: &EmbedsQuery) -> ChatResult<EmbedPage> {
        let conn = self.conn()?;
        let limit = match query.limit {
            0 => PAGE_DEFAULT,
            limit => limit.min(PAGE_MAX),
        } as usize;
        let kinds = query
            .kinds
            .iter()
            .map(|kind| format!("'{}'", label(kind)))
            .collect::<Vec<_>>()
            .join(", ");
        let kind_filter = if kinds.is_empty() {
            String::new()
        } else {
            format!("AND e.kind IN ({kinds})")
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT e.message_id, e.position, e.nonce, e.body, m.seq, m.ts, m.sender FROM message_embeds e JOIN messages m ON m.id = e.message_id WHERE e.conversation_id = ?1 AND m.seq < ?2 {kind_filter} ORDER BY m.seq DESC, e.position LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(
                params![
                    id.as_bytes(),
                    query.before.unwrap_or(i64::MAX),
                    limit as i64 + 1
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let more = rows.len() > limit;
        let items = rows
            .into_iter()
            .take(limit)
            .filter_map(|(message_id, position, nonce, body, seq, ts, sender)| {
                let aad = aad(message_id, position);
                let plain = seal::open(&self.keys.body, &aad, &nonce, &body).ok()?;
                let embed = from_json::<Embed>(&plain).ok()?;
                Some(EmbedHit {
                    seq,
                    ts,
                    sender,
                    embed,
                })
            })
            .collect();
        Ok(EmbedPage { items, more })
    }
}

pub fn replace(
    tx: &Connection,
    keys: &StoreKeys,
    message_id: i64,
    conv: ConvId,
    embeds: &[Embed],
) -> ChatResult<()> {
    tx.prepare_cached("DELETE FROM message_embeds WHERE message_id = ?1")?
        .execute([message_id])?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO message_embeds (message_id, position, conversation_id, kind, nonce, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (position, embed) in embeds.iter().enumerate() {
        let position = position as u32;
        let sealed = seal::seal(&keys.body, &aad(message_id, position), &to_json(embed)?)?;
        insert.execute(params![
            message_id,
            position,
            conv.as_bytes(),
            label(embed.kind),
            sealed.nonce,
            sealed.body
        ])?;
    }
    Ok(())
}

fn aad(message_id: i64, position: u32) -> Vec<u8> {
    seal::tagged_aad(
        "embed",
        &[&message_id.to_be_bytes(), &position.to_be_bytes()],
    )
}
