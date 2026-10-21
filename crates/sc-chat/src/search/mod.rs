pub(crate) mod index;
mod snippet;
mod tokens;

use std::sync::Arc;

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::contracts::{ChatResult, ConvId, SearchHit, SearchQuery, Seq};
use crate::store::Store;

pub const PREFIX_MIN: usize = 2;
pub const PREFIX_MAX: usize = 16;
pub const TOKEN_BYTES: usize = 12;

const HITS_DEFAULT: u32 = 50;
const HITS_MAX: u32 = 200;

pub struct Search {
    store: Arc<Store>,
}

impl Search {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn index(&self, conversation_id: ConvId, seq: Seq, text: &str) -> ChatResult<()> {
        let conn = self.store.conn()?;
        let Some(rowid) = rowid(&conn, conversation_id, seq)? else {
            return Ok(());
        };
        index::replace(&conn, &self.store.keys().search, rowid, Some(text))
    }

    pub fn remove(&self, conversation_id: ConvId, seq: Seq) -> ChatResult<()> {
        let conn = self.store.conn()?;
        let Some(rowid) = rowid(&conn, conversation_id, seq)? else {
            return Ok(());
        };
        index::remove(&conn, rowid)
    }

    pub fn query(&self, query: &SearchQuery) -> ChatResult<Vec<SearchHit>> {
        let Some(matched) = tokens::query(&self.store.keys().search, &query.text)? else {
            return Ok(Vec::new());
        };
        let limit = match query.limit {
            0 => HITS_DEFAULT,
            limit => limit.min(HITS_MAX),
        };
        let conn = self.store.conn()?;
        let found: Vec<(ConvId, Seq)> = conn
            .prepare_cached(
                "SELECT m.conversation_id, m.seq FROM search JOIN messages m ON m.id = search.rowid WHERE search MATCH ?1 AND (?2 IS NULL OR m.conversation_id = ?2) ORDER BY m.ts DESC, m.id DESC LIMIT ?3",
            )?
            .query_map(
                params![
                    matched,
                    query.conversation_id.map(|id| *id.as_bytes()),
                    limit
                ],
                |row| Ok((Uuid::from_bytes(row.get(0)?), row.get(1)?)),
            )?
            .collect::<Result<_, _>>()?;
        let mut hits = Vec::with_capacity(found.len());
        for (conversation_id, seq) in found {
            let Some(message) = self.store.message_at(&conn, conversation_id, seq)? else {
                continue;
            };
            hits.push(SearchHit {
                conversation_id,
                seq,
                ts: message.ts,
                sender: message.sender.clone(),
                snippet: snippet::around(&message, &query.text),
            });
        }
        Ok(hits)
    }
}

fn rowid(conn: &Connection, conversation_id: ConvId, seq: Seq) -> ChatResult<Option<i64>> {
    Ok(conn
        .prepare_cached("SELECT id FROM messages WHERE conversation_id = ?1 AND seq = ?2")?
        .query_row(params![conversation_id.as_bytes(), seq], |row| row.get(0))
        .optional()?)
}
