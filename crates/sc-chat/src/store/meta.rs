use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use super::{Store, SyncState};
use crate::contracts::{ChatResult, ConvId, Seq, UserId};

const POS: &str = "pos";
const INC: &str = "inc";
const ME: &str = "me";

pub fn get(conn: &Connection, key: &str) -> ChatResult<Option<Vec<u8>>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &[u8]) -> ChatResult<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, key: &str) -> ChatResult<()> {
    conn.execute("DELETE FROM meta WHERE key = ?1", [key])?;
    Ok(())
}

pub fn me(conn: &Connection) -> ChatResult<Option<UserId>> {
    Ok(get(conn, ME)?.and_then(|raw| String::from_utf8(raw).ok()))
}

impl Store {
    pub fn me(&self) -> ChatResult<Option<UserId>> {
        me(&*self.conn()?)
    }

    pub fn set_me(&self, user_id: &str) -> ChatResult<()> {
        set(&*self.conn()?, ME, user_id.as_bytes())
    }

    pub fn sync_state(&self) -> ChatResult<SyncState> {
        let conn = self.conn()?;
        let pos = get(&conn, POS)?
            .and_then(|raw| <[u8; 8]>::try_from(raw.as_slice()).ok())
            .map(u64::from_be_bytes)
            .unwrap_or_default();
        let inc = get(&conn, INC)?.and_then(|raw| Uuid::from_slice(&raw).ok());
        Ok(SyncState { pos, inc })
    }

    pub fn set_sync_state(&self, state: SyncState) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        set(&tx, POS, &state.pos.to_be_bytes())?;
        match state.inc {
            Some(inc) => set(&tx, INC, inc.as_bytes())?,
            None => delete(&tx, INC)?,
        }
        tx.commit()?;
        Ok(())
    }

    pub fn known(&self, limit: usize) -> ChatResult<Vec<(ConvId, Seq)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, last_seq FROM conversations WHERE state IN ('active', 'invited') ORDER BY last_at DESC, id LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok((super::codec::uuid_at(row, 0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
