use std::collections::HashSet;

use rusqlite::{Connection, params};

use super::items::{self, Item};
use super::purge::{self, finish, secure};
use super::{Store, StoreKeys, messages};
use crate::contracts::{ChatResult, ConvId, Seq};

const LOCAL_SPAN: Seq = 1 << 40;

impl Store {
    pub fn keep_local_above(&self, server_last: &[(ConvId, Seq)]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        secure(&conn)?;
        let tx = conn.transaction()?;
        let mut dropped = false;
        for (id, last) in server_last {
            dropped |= keep_block(&tx, &self.keys, *id, *last)?;
        }
        tx.commit()?;
        if dropped {
            finish(&conn)?;
        }
        Ok(())
    }
}

fn keep_block(tx: &Connection, keys: &StoreKeys, id: ConvId, last: Seq) -> ChatResult<bool> {
    tx.execute(
        "UPDATE conversations SET last_seq = ?2 WHERE id = ?1",
        params![id.as_bytes(), last],
    )?;
    tx.execute(
        "DELETE FROM outbox WHERE conversation_id = ?1 AND target_seq > ?2",
        params![id.as_bytes(), last],
    )?;
    let rows = items::load(
        tx,
        keys,
        "conversation_id = ?1 AND local_only = 0 AND seq > ?2 ORDER BY seq",
        params![id.as_bytes(), last],
    )?;
    if rows.is_empty() {
        return Ok(false);
    }
    let shift = block_shift(tx, id)?;
    let moved: HashSet<Seq> = rows.iter().map(|row| row.message.seq).collect();
    let mut orphaned = Vec::new();
    for row in &rows {
        match row.message.target_seq {
            Some(target) if !moved.contains(&target) => {
                purge::drop_row(tx, row.id)?;
                orphaned.push(target);
            }
            target => rewrite(
                tx,
                keys,
                row,
                row.message.seq - shift,
                target.map(|seq| seq - shift),
            )?,
        }
    }
    for target in &orphaned {
        messages::refresh(tx, keys, id, *target)?;
    }
    Ok(!orphaned.is_empty())
}

fn block_shift(tx: &Connection, id: ConvId) -> ChatResult<Seq> {
    let lowest: Option<Seq> = tx.query_row(
        "SELECT min(seq) FROM messages WHERE conversation_id = ?1 AND seq < 0",
        [id.as_bytes()],
        |row| row.get(0),
    )?;
    let blocks = lowest.map(|seq| -seq / LOCAL_SPAN + 1).unwrap_or_default();
    Ok((blocks + 1) * LOCAL_SPAN)
}

fn rewrite(
    tx: &Connection,
    keys: &StoreKeys,
    row: &Item,
    seq: Seq,
    target_seq: Option<Seq>,
) -> ChatResult<()> {
    let message = &row.message;
    let (kind, sealed) = items::seal_content(keys, message.conversation_id, seq, &message.content)?;
    let (nonce, body) = match sealed {
        Some(sealed) => (Some(sealed.nonce), Some(sealed.body)),
        None => (None, None),
    };
    tx.prepare_cached(
        "UPDATE messages SET seq = ?2, target_seq = ?3, local_only = 1, content = ?4, nonce = ?5, body = ?6 WHERE id = ?1",
    )?
    .execute(params![row.id, seq, target_seq, kind, nonce, body])?;
    Ok(())
}
