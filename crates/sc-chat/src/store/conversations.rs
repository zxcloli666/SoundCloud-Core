use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use super::codec::{Label, from_json, label, to_json, uuid_at};
use super::project::Context;
use super::{Store, StoreKeys, history, meta, people, seal};
use crate::contracts::{
    ChatResult, ConvId, ConversationPage, ConversationSummary, ListFilter, ListQuery, Message,
    MessageBody, MessagePreview, PreviewKind, Seq, SystemKind,
};

const PAGE_DEFAULT: u32 = 50;
const PAGE_MAX: u32 = 200;
const PREVIEW_CHARS: usize = 120;

const UPSERT: &str = "INSERT INTO conversations (id, kind, role, state, accepted, member_count, peer, room_code, last_seq, last_at, last_read_seq, unread, mentions, muted_until, notify, pinned_at, archived, ttl_s, history_locked, meta_nonce, meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21) ON CONFLICT (id) DO UPDATE SET kind = excluded.kind, role = excluded.role, state = excluded.state, accepted = excluded.accepted, member_count = excluded.member_count, peer = excluded.peer, room_code = excluded.room_code, last_seq = excluded.last_seq, last_at = excluded.last_at, last_read_seq = excluded.last_read_seq, unread = excluded.unread, mentions = excluded.mentions, muted_until = excluded.muted_until, notify = excluded.notify, pinned_at = excluded.pinned_at, archived = excluded.archived, ttl_s = excluded.ttl_s, history_locked = excluded.history_locked, meta_nonce = excluded.meta_nonce, meta = excluded.meta";

const SELECT: &str = "SELECT id, kind, role, state, accepted, member_count, peer, room_code, last_seq, last_at, last_read_seq, unread, mentions, muted_until, notify, pinned_at, archived, ttl_s, history_locked, meta_nonce, meta, EXISTS (SELECT 1 FROM drafts d WHERE d.conversation_id = c.id) FROM conversations c";

const ORDER: &str = "ORDER BY pinned_at IS NULL, pinned_at DESC, last_at DESC, id";

#[derive(Default, Serialize, Deserialize)]
struct Sealed {
    title: Option<String>,
    avatar_url: Option<String>,
    preview: Option<MessagePreview>,
}

struct Head {
    summary: ConversationSummary,
    peer: Option<String>,
    meta: Option<(Vec<u8>, Vec<u8>)>,
}

impl Store {
    pub fn put_conversations(&self, items: &[ConversationSummary]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for item in items {
            if let Some(peer) = &item.peer {
                people::put_user(&tx, &self.keys, peer)?;
            }
            let sealed = Sealed {
                title: item.title.clone(),
                avatar_url: item.avatar_url.clone(),
                preview: item.preview.clone(),
            };
            let sealed = seal::seal(&self.keys.body, &aad(item.id), &to_json(&sealed)?)?;
            tx.prepare_cached(UPSERT)?.execute(params![
                item.id.as_bytes(),
                label(item.kind),
                label(item.role),
                label(item.state),
                item.accepted,
                item.member_count,
                item.peer.as_ref().map(|peer| peer.id.as_str()),
                item.room_code,
                item.last_seq,
                item.last_at,
                item.last_read_seq,
                item.unread,
                item.mentions,
                item.muted_until,
                label(item.notify),
                item.pinned_at,
                item.archived,
                item.ttl_s,
                item.history_locked,
                sealed.nonce,
                sealed.body,
            ])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn conversations(&self, query: &ListQuery) -> ChatResult<ConversationPage> {
        let conn = self.conn()?;
        let limit = match query.limit {
            0 => PAGE_DEFAULT,
            limit => limit.min(PAGE_MAX),
        } as usize;
        let offset = query
            .cursor
            .as_deref()
            .and_then(|cursor| cursor.parse::<usize>().ok())
            .unwrap_or_default();
        let sql = format!(
            "{SELECT} WHERE {} {ORDER} LIMIT ?1 OFFSET ?2",
            filter(query.filter)
        );
        let heads = conn
            .prepare_cached(&sql)?
            .query_map(params![limit as i64 + 1, offset as i64], head)?
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = (heads.len() > limit).then(|| (offset + limit).to_string());
        let items = heads
            .into_iter()
            .take(limit)
            .map(|head| summary(&conn, &self.keys, head))
            .collect::<ChatResult<_>>()?;
        Ok(ConversationPage { items, next_cursor })
    }

    pub fn conversation(&self, id: ConvId) -> ChatResult<Option<ConversationSummary>> {
        let conn = self.conn()?;
        let head = conn
            .prepare_cached(&format!("{SELECT} WHERE id = ?1"))?
            .query_map([id.as_bytes()], head)?
            .next()
            .transpose()?;
        head.map(|head| summary(&conn, &self.keys, head))
            .transpose()
    }

    pub fn hide_before(&self, id: ConvId, seq: Seq) -> ChatResult<()> {
        let mut conn = self.conn()?;
        super::purge::secure(&conn)?;
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE conversations SET hidden_before_seq = max(hidden_before_seq, ?2) WHERE id = ?1",
            params![id.as_bytes(), seq],
        )?;
        let rows: Vec<i64> = tx
            .prepare_cached(
                "SELECT id FROM messages WHERE conversation_id = ?1 AND local_only = 0 AND seq <= ?2",
            )?
            .query_map(params![id.as_bytes(), seq], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        for row in rows {
            super::purge::drop_row(&tx, row)?;
        }
        forget_preview(&tx, &self.keys, id, |preview| preview <= seq)?;
        tx.commit()?;
        super::purge::finish(&conn)
    }
}

pub fn forget_preview(
    tx: &Connection,
    keys: &StoreKeys,
    id: ConvId,
    gone: impl Fn(Seq) -> bool,
) -> ChatResult<()> {
    let found: Option<(Vec<u8>, Vec<u8>)> = tx
        .prepare_cached(
            "SELECT meta_nonce, meta FROM conversations WHERE id = ?1 AND meta_nonce IS NOT NULL AND meta IS NOT NULL",
        )?
        .query_row([id.as_bytes()], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    let Some((nonce, body)) = found else {
        return Ok(());
    };
    let Some(mut sealed) = seal::open(&keys.body, &aad(id), &nonce, &body)
        .ok()
        .and_then(|plain| from_json::<Sealed>(&plain).ok())
    else {
        return Ok(());
    };
    if !sealed
        .preview
        .as_ref()
        .is_some_and(|preview| gone(preview.seq))
    {
        return Ok(());
    }
    sealed.preview = None;
    let resealed = seal::seal(&keys.body, &aad(id), &to_json(&sealed)?)?;
    tx.prepare_cached("UPDATE conversations SET meta_nonce = ?2, meta = ?3 WHERE id = ?1")?
        .execute(params![id.as_bytes(), resealed.nonce, resealed.body])?;
    Ok(())
}

fn filter(filter: ListFilter) -> &'static str {
    match filter {
        ListFilter::All => "accepted = 1 AND archived = 0",
        ListFilter::Direct => "accepted = 1 AND archived = 0 AND kind IN ('direct', 'self')",
        ListFilter::Groups => "accepted = 1 AND archived = 0 AND kind = 'group'",
        ListFilter::Unread => "accepted = 1 AND archived = 0 AND unread > 0",
        ListFilter::Requests => "accepted = 0 AND state IN ('active', 'invited')",
        ListFilter::Archived => "accepted = 1 AND archived = 1",
    }
}

fn head(row: &Row<'_>) -> rusqlite::Result<Head> {
    Ok(Head {
        summary: ConversationSummary {
            id: uuid_at(row, 0)?,
            kind: row.get::<_, Label<_>>(1)?.0,
            role: row.get::<_, Label<_>>(2)?.0,
            state: row.get::<_, Label<_>>(3)?.0,
            accepted: row.get(4)?,
            member_count: row.get(5)?,
            room_code: row.get(7)?,
            last_seq: row.get(8)?,
            last_at: row.get(9)?,
            last_read_seq: row.get(10)?,
            unread: row.get(11)?,
            mentions: row.get(12)?,
            muted_until: row.get(13)?,
            notify: row.get::<_, Label<_>>(14)?.0,
            pinned_at: row.get(15)?,
            archived: row.get(16)?,
            ttl_s: row.get(17)?,
            history_locked: row.get(18)?,
            has_draft: row.get(21)?,
            title: None,
            avatar_url: None,
            peer: None,
            preview: None,
        },
        peer: row.get(6)?,
        meta: match (row.get(19)?, row.get(20)?) {
            (Some(nonce), Some(body)) => Some((nonce, body)),
            _ => None,
        },
    })
}

fn summary(conn: &Connection, keys: &StoreKeys, head: Head) -> ChatResult<ConversationSummary> {
    let mut summary = head.summary;
    let sealed = match head.meta {
        Some((nonce, body)) => seal::open(&keys.body, &aad(summary.id), &nonce, &body)
            .ok()
            .and_then(|plain| from_json::<Sealed>(&plain).ok())
            .unwrap_or_default(),
        None => Sealed::default(),
    };
    summary.title = sealed.title;
    summary.avatar_url = sealed.avatar_url;
    summary.peer = match head.peer {
        Some(peer) => people::user(conn, keys, &peer)?,
        None => None,
    };
    let me = meta::me(conn)?;
    let context = Context {
        me: me.as_deref(),
        marks: &[],
    };
    summary.preview = history::latest(conn, keys, summary.id, context)?
        .map(|message| preview(&message))
        .or(sealed.preview);
    Ok(summary)
}

pub fn preview(message: &Message) -> MessagePreview {
    let (kind, text) = match &message.body {
        MessageBody::Text(text) if !text.text.trim().is_empty() => (
            PreviewKind::Text,
            text.text.chars().take(PREVIEW_CHARS).collect(),
        ),
        MessageBody::Text(_) => match message.embeds.first() {
            Some(embed) => (PreviewKind::Embed, embed.title.clone()),
            None => (PreviewKind::Text, String::new()),
        },
        MessageBody::System(event) if event.kind == SystemKind::Deleted => {
            (PreviewKind::Deleted, String::new())
        }
        MessageBody::System(_) => (PreviewKind::System, String::new()),
        MessageBody::Deleted => (PreviewKind::Deleted, String::new()),
        MessageBody::Locked => (PreviewKind::Locked, String::new()),
        MessageBody::Unknown => (PreviewKind::Unknown, String::new()),
    };
    MessagePreview {
        seq: message.seq.unwrap_or_default(),
        sender: message.sender.clone(),
        kind,
        text,
        ts: message.ts,
        expires_at: message.expires_at,
    }
}

fn aad(id: ConvId) -> Vec<u8> {
    seal::tagged_aad("conversation", &[id.as_bytes()])
}
