use std::sync::atomic::Ordering;

use rusqlite::{Connection, OptionalExtension, params};

use super::codec::uuid_at;
use super::{Store, StoreKeys, conversations, messages, meta};
use crate::contracts::{ChatResult, ConvId, GoneMessage, Millis, Seq};
use crate::search::index;

pub const COMPACTION_GAP_MS: Millis = 60_000;

const SEARCH_OWED: &str = "search_owed";

const CONVERSATION_TABLES: [&str; 8] = [
    "message_embeds",
    "messages",
    "members",
    "marks",
    "conversation_keys",
    "drafts",
    "outbox",
    "hidden_messages",
];

const WIPED_TABLES: [&str; 10] = [
    "message_embeds",
    "messages",
    "members",
    "marks",
    "conversation_keys",
    "drafts",
    "outbox",
    "hidden_messages",
    "conversations",
    "users",
];

const ORPHAN_USERS: &str = "DELETE FROM users WHERE id NOT IN (SELECT peer FROM conversations WHERE peer IS NOT NULL) AND id NOT IN (SELECT user_id FROM members) AND id NOT IN (SELECT sender FROM messages)";

impl Store {
    pub fn purge_conversations(&self, ids: &[ConvId]) -> ChatResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn()?;
        secure(&conn)?;
        let tx = conn.transaction()?;
        let mut dropped = false;
        for id in ids {
            dropped |= drop_conversation(&tx, *id)?;
        }
        tx.execute(ORPHAN_USERS, [])?;
        tx.commit()?;
        if dropped {
            finish(&conn)?;
        }
        Ok(())
    }

    pub fn purge_messages(&self, gone: &[GoneMessage]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        if drop_messages(&mut conn, &self.keys, gone)? {
            finish(&conn)?;
        }
        Ok(())
    }

    pub fn hide_messages(&self, hidden: &[GoneMessage]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        for message in hidden {
            conn.prepare_cached(
                "INSERT INTO hidden_messages (conversation_id, seq) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            )?
            .execute(params![message.conversation_id.as_bytes(), message.seq])?;
        }
        if drop_messages(&mut conn, &self.keys, hidden)? {
            finish(&conn)?;
        }
        Ok(())
    }

    pub fn purge_expired(&self, now: Millis) -> ChatResult<Vec<GoneMessage>> {
        let mut conn = self.conn()?;
        let expired = expired(&conn, now)?;
        if drop_messages(&mut conn, &self.keys, &expired)? {
            meta::set(&conn, SEARCH_OWED, &[1])?;
            checkpoint(&conn)?;
        }
        let owed = meta::get(&conn, SEARCH_OWED)?.is_some();
        if owed
            && now.saturating_sub(self.compacted_at.load(Ordering::Relaxed)) >= COMPACTION_GAP_MS
        {
            finish(&conn)?;
            self.compacted_at.store(now, Ordering::Relaxed);
        }
        Ok(expired)
    }

    pub fn next_expiry(&self) -> ChatResult<Option<Millis>> {
        Ok(self.conn()?.query_row(
            "SELECT min(expires_at) FROM messages WHERE expires_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn wipe(&self) -> ChatResult<()> {
        let mut conn = self.conn()?;
        secure(&conn)?;
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO search (search) VALUES ('delete-all')", [])?;
        for table in WIPED_TABLES {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        tx.execute("DELETE FROM meta WHERE key <> 'key_check'", [])?;
        tx.commit()?;
        conn.execute("INSERT INTO search (search) VALUES ('optimize')", [])?;
        conn.execute_batch("VACUUM")?;
        checkpoint(&conn)
    }
}

fn expired(conn: &Connection, now: Millis) -> ChatResult<Vec<GoneMessage>> {
    let mut stmt = conn.prepare_cached(
        "SELECT conversation_id, seq FROM messages WHERE expires_at IS NOT NULL AND expires_at <= ?1 ORDER BY expires_at",
    )?;
    let rows = stmt.query_map([now], |row| {
        Ok(GoneMessage {
            conversation_id: uuid_at(row, 0)?,
            seq: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn drop_messages(
    conn: &mut Connection,
    keys: &StoreKeys,
    gone: &[GoneMessage],
) -> ChatResult<bool> {
    if gone.is_empty() {
        return Ok(false);
    }
    secure(conn)?;
    let tx = conn.transaction()?;
    let mut dropped = false;
    for message in gone {
        dropped |= drop_message(&tx, keys, message.conversation_id, message.seq)?;
    }
    tx.commit()?;
    Ok(dropped)
}

fn drop_conversation(tx: &Connection, id: ConvId) -> ChatResult<bool> {
    let mut dropped = tx.execute(
        "DELETE FROM search WHERE rowid IN (SELECT id FROM messages WHERE conversation_id = ?1)",
        [id.as_bytes()],
    )?;
    for table in CONVERSATION_TABLES {
        dropped += tx.execute(
            &format!("DELETE FROM {table} WHERE conversation_id = ?1"),
            [id.as_bytes()],
        )?;
    }
    dropped += tx.execute("DELETE FROM conversations WHERE id = ?1", [id.as_bytes()])?;
    Ok(dropped > 0)
}

fn drop_message(tx: &Connection, keys: &StoreKeys, conv: ConvId, seq: Seq) -> ChatResult<bool> {
    let found: Option<(i64, Option<Seq>)> = tx
        .prepare_cached(
            "SELECT id, target_seq FROM messages WHERE conversation_id = ?1 AND seq = ?2",
        )?
        .query_row(params![conv.as_bytes(), seq], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?;
    let Some((id, target)) = found else {
        return Ok(false);
    };
    let modifiers: Vec<i64> = tx
        .prepare_cached("SELECT id FROM messages WHERE conversation_id = ?1 AND target_seq = ?2")?
        .query_map(params![conv.as_bytes(), seq], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    for row in modifiers.into_iter().chain([id]) {
        drop_row(tx, row)?;
    }
    conversations::forget_preview(tx, keys, conv, |preview| preview == seq)?;
    if let Some(target) = target {
        messages::refresh(tx, keys, conv, target)?;
    }
    Ok(true)
}

pub fn drop_row(tx: &Connection, id: i64) -> ChatResult<()> {
    index::remove(tx, id)?;
    tx.prepare_cached("DELETE FROM message_embeds WHERE message_id = ?1")?
        .execute([id])?;
    tx.prepare_cached("DELETE FROM messages WHERE id = ?1")?
        .execute([id])?;
    Ok(())
}

pub fn secure(conn: &Connection) -> ChatResult<()> {
    conn.pragma_update(None, "secure_delete", "ON")?;
    Ok(())
}

pub fn finish(conn: &Connection) -> ChatResult<()> {
    meta::delete(conn, SEARCH_OWED)?;
    conn.execute("INSERT INTO search (search) VALUES ('optimize')", [])?;
    checkpoint(conn)
}

fn checkpoint(conn: &Connection) -> ChatResult<()> {
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    Ok(())
}
