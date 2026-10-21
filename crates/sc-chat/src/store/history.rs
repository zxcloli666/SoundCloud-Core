use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension, params};

use super::items::{self, Item};
use super::project::{self, Context};
use super::{Store, StoreKeys, StoredContent, content, meta, people};
use crate::contracts::{ChatResult, ConvId, HistoryPage, HistoryQuery, Message, Seq};
use crate::envelope::Content;

const PAGE_DEFAULT: u32 = 50;
const PAGE_MAX: u32 = 200;
const VISIBLE: &str = "class IN ('post', 'system')";
const SERVER: &str = "local_only = 0 AND seq > 0";
const LOCAL: &str = "local_only = 1 AND seq < 0";

impl Store {
    pub fn messages(&self, id: ConvId, query: &HistoryQuery) -> ChatResult<HistoryPage> {
        let conn = self.conn()?;
        let me = meta::me(&conn)?;
        let marks = people::marks(&conn, id)?;
        let context = Context {
            me: me.as_deref(),
            marks: &marks,
        };
        let limit = match query.limit {
            0 => PAGE_DEFAULT,
            limit => limit.min(PAGE_MAX),
        } as usize;
        let (rows, more) = server_page(&conn, &self.keys, id, query, limit)?;
        let cursor = query.before.is_some() || query.after.is_some();
        let locals = if rows.is_empty() && cursor {
            Vec::new()
        } else {
            local_rows(&conn, &self.keys, id, &rows)?
        };
        let rows = merge(rows, locals);
        let messages = project_rows(&conn, &self.keys, id, &rows, context)?;
        Ok(HistoryPage { messages, more })
    }

    pub(crate) fn message_at(
        &self,
        conn: &Connection,
        id: ConvId,
        seq: Seq,
    ) -> ChatResult<Option<Message>> {
        let me = meta::me(conn)?;
        let context = Context {
            me: me.as_deref(),
            marks: &[],
        };
        let rows: Vec<Item> = items::by_seq(conn, &self.keys, id, seq)?
            .into_iter()
            .collect();
        Ok(project_rows(conn, &self.keys, id, &rows, context)?.pop())
    }

    pub fn pins(&self, id: ConvId) -> ChatResult<Vec<Seq>> {
        let conn = self.conn()?;
        let rows = items::load(
            &conn,
            &self.keys,
            "conversation_id = ?1 AND class = 'pin' ORDER BY seq",
            [id.as_bytes()],
        )?;
        let mut pinned: Vec<Seq> = Vec::new();
        for row in rows {
            let StoredContent::Open(bytes) = &row.message.content else {
                continue;
            };
            let Content::Pin { target, on } = content::decode(bytes) else {
                continue;
            };
            let Some(target) = row
                .message
                .target_seq
                .filter(|seq| project::aims_at(&row.message, target, Some(*seq)))
            else {
                continue;
            };
            pinned.retain(|seq| *seq != target);
            if on {
                pinned.push(target);
            }
        }
        Ok(pinned)
    }
}

pub fn latest(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    context: Context<'_>,
) -> ChatResult<Option<Message>> {
    let rows = items::load(
        conn,
        keys,
        &format!("conversation_id = ?1 AND {VISIBLE} ORDER BY seq DESC LIMIT 1"),
        [id.as_bytes()],
    )?;
    Ok(project_rows(conn, keys, id, &rows, context)?.pop())
}

fn server_page(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    query: &HistoryQuery,
    limit: usize,
) -> ChatResult<(Vec<Item>, bool)> {
    let fetch = limit as i64 + 1;
    if let Some(after) = query.after {
        let after = anchor(conn, id, after, "max", "<=")?.unwrap_or_default();
        let mut rows = items::load(
            conn,
            keys,
            &format!(
                "conversation_id = ?1 AND {SERVER} AND {VISIBLE} AND seq > ?2 ORDER BY seq LIMIT ?3"
            ),
            params![id.as_bytes(), after, fetch],
        )?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        return Ok((rows, more));
    }
    let before = match query.before {
        Some(before) => anchor(conn, id, before, "min", ">=")?.unwrap_or(i64::MAX),
        None => i64::MAX,
    };
    let mut rows = items::load(
        conn,
        keys,
        &format!(
            "conversation_id = ?1 AND {SERVER} AND {VISIBLE} AND seq < ?2 ORDER BY seq DESC LIMIT ?3"
        ),
        params![id.as_bytes(), before, fetch],
    )?;
    let more = rows.len() > limit;
    rows.truncate(limit);
    rows.reverse();
    Ok((rows, more))
}

fn anchor(
    conn: &Connection,
    id: ConvId,
    cursor: Seq,
    pick: &str,
    side: &str,
) -> ChatResult<Option<Seq>> {
    if cursor >= 0 {
        return Ok(Some(cursor));
    }
    Ok(conn
        .query_row(
            &format!(
                "SELECT {pick}(seq) FROM messages WHERE conversation_id = ?1 AND {SERVER} AND {VISIBLE} AND ts {side} (SELECT ts FROM messages WHERE conversation_id = ?1 AND seq = ?2)"
            ),
            params![id.as_bytes(), cursor],
            |row| row.get(0),
        )
        .optional()?
        .flatten())
}

fn local_rows(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    rows: &[Item],
) -> ChatResult<Vec<Item>> {
    let (first, last) = match (rows.first(), rows.last()) {
        (Some(first), Some(last)) => (&first.message, &last.message),
        _ => {
            return items::load(
                conn,
                keys,
                &format!("conversation_id = ?1 AND {LOCAL} AND {VISIBLE} ORDER BY ts, seq"),
                [id.as_bytes()],
            );
        }
    };
    let older: bool = conn.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM messages WHERE conversation_id = ?1 AND {SERVER} AND {VISIBLE} AND seq < ?2)"),
        params![id.as_bytes(), first.seq],
        |row| row.get(0),
    )?;
    let newer: Option<i64> = conn
        .query_row(
            &format!("SELECT ts FROM messages WHERE conversation_id = ?1 AND {SERVER} AND {VISIBLE} AND seq > ?2 ORDER BY seq LIMIT 1"),
            params![id.as_bytes(), last.seq],
            |row| row.get(0),
        )
        .optional()?;
    let from = if older { first.ts } else { i64::MIN };
    items::load(
        conn,
        keys,
        &format!(
            "conversation_id = ?1 AND {LOCAL} AND {VISIBLE} AND ts >= ?2 AND ts < ?3 ORDER BY ts, seq"
        ),
        params![id.as_bytes(), from, newer.unwrap_or(i64::MAX)],
    )
}

fn merge(rows: Vec<Item>, locals: Vec<Item>) -> Vec<Item> {
    let mut merged = Vec::with_capacity(rows.len() + locals.len());
    let mut locals = locals.into_iter().peekable();
    for row in rows {
        while let Some(local) = locals.next_if(|local| local.message.ts < row.message.ts) {
            merged.push(local);
        }
        merged.push(row);
    }
    merged.extend(locals);
    merged
}

fn project_rows(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    rows: &[Item],
    context: Context<'_>,
) -> ChatResult<Vec<Message>> {
    let modifiers = modifiers(conn, keys, id, rows)?;
    let mut grouped: HashMap<Seq, Vec<&Item>> = HashMap::new();
    for modifier in &modifiers {
        if let Some(target) = modifier.message.target_seq {
            grouped.entry(target).or_default().push(modifier);
        }
    }
    Ok(rows
        .iter()
        .map(|row| {
            let targeting = grouped
                .get(&row.message.seq)
                .map(Vec::as_slice)
                .unwrap_or_default();
            project::message(row, targeting, context)
        })
        .collect())
}

fn modifiers(
    conn: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    rows: &[Item],
) -> ChatResult<Vec<Item>> {
    let mut found = Vec::new();
    for local in [false, true] {
        let seqs = rows
            .iter()
            .filter(|row| row.message.local_only == local)
            .map(|row| row.message.seq);
        let (Some(from), Some(to)) = (seqs.clone().min(), seqs.max()) else {
            continue;
        };
        found.extend(items::targeting(conn, keys, id, from, to)?);
    }
    Ok(found)
}
