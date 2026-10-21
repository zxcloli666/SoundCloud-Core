use rusqlite::{Connection, OptionalExtension, params};

use super::codec::{Label, from_json, label, to_json};
use super::{Store, StoreKeys, seal};
use crate::contracts::{ChatResult, ConvId, Member, PeerMark, UserId, UserRef};

impl Store {
    pub fn put_users(&self, users: &[UserRef]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for user in users {
            put_user(&tx, &self.keys, user)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn users(&self, ids: &[UserId]) -> ChatResult<Vec<UserRef>> {
        let conn = self.conn()?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.extend(user(&conn, &self.keys, id)?);
        }
        Ok(found)
    }

    pub fn put_members(&self, id: ConvId, members: &[Member]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM members WHERE conversation_id = ?1",
            [id.as_bytes()],
        )?;
        for member in members {
            put_user(&tx, &self.keys, &member.user)?;
            tx.prepare_cached(
                "INSERT INTO members (conversation_id, user_id, role, state) VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![
                id.as_bytes(),
                member.user.id,
                label(member.role),
                label(member.state)
            ])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn members(&self, id: ConvId) -> ChatResult<Vec<Member>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare_cached(
            "SELECT user_id, role, state FROM members WHERE conversation_id = ?1 ORDER BY CASE role WHEN 'owner' THEN 0 WHEN 'admin' THEN 1 ELSE 2 END, user_id",
        )?;
        let rows = stmt
            .query_map([id.as_bytes()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Label<_>>(1)?.0,
                    row.get::<_, Label<_>>(2)?.0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(user_id, role, state)| {
                Ok(Member {
                    user: user(&conn, &self.keys, &user_id)?.unwrap_or_else(|| bare(user_id)),
                    role,
                    state,
                    accepted: false,
                    joined_seq: 0,
                    invited_by: None,
                })
            })
            .collect()
    }

    pub fn put_marks(&self, id: ConvId, marks: &[PeerMark]) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for mark in marks {
            tx.prepare_cached(
                "INSERT INTO marks (conversation_id, user_id, read_seq, delivered_seq) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (conversation_id, user_id) DO UPDATE SET read_seq = max(read_seq, excluded.read_seq), delivered_seq = max(delivered_seq, excluded.delivered_seq)",
            )?
            .execute(params![
                id.as_bytes(),
                mark.user_id,
                mark.read_seq,
                mark.delivered_seq.max(mark.read_seq)
            ])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn marks(&self, id: ConvId) -> ChatResult<Vec<PeerMark>> {
        marks(&*self.conn()?, id)
    }
}

pub fn marks(conn: &Connection, id: ConvId) -> ChatResult<Vec<PeerMark>> {
    let mut stmt = conn.prepare_cached(
        "SELECT user_id, read_seq, delivered_seq FROM marks WHERE conversation_id = ?1 ORDER BY user_id",
    )?;
    let rows = stmt.query_map([id.as_bytes()], |row| {
        Ok(PeerMark {
            user_id: row.get(0)?,
            read_seq: row.get(1)?,
            delivered_seq: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn put_user(tx: &Connection, keys: &StoreKeys, user: &UserRef) -> ChatResult<()> {
    let sealed = seal::seal(&keys.body, &aad(&user.id), &to_json(user)?)?;
    tx.prepare_cached(
        "INSERT INTO users (id, updated_at, nonce, body) VALUES (?1, CAST(unixepoch('subsec') * 1000 AS INTEGER), ?2, ?3) ON CONFLICT (id) DO UPDATE SET updated_at = excluded.updated_at, nonce = excluded.nonce, body = excluded.body",
    )?
    .execute(params![user.id, sealed.nonce, sealed.body])?;
    Ok(())
}

pub fn user(conn: &Connection, keys: &StoreKeys, id: &str) -> ChatResult<Option<UserRef>> {
    let found: Option<(Vec<u8>, Vec<u8>)> = conn
        .prepare_cached("SELECT nonce, body FROM users WHERE id = ?1")?
        .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional()?;
    let Some((nonce, body)) = found else {
        return Ok(None);
    };
    let plain = seal::open(&keys.body, &aad(id), &nonce, &body)?;
    Ok(Some(from_json(&plain)?))
}

fn bare(id: UserId) -> UserRef {
    UserRef {
        name: id.clone(),
        id,
        avatar_url: None,
        permalink: None,
    }
}

fn aad(id: &str) -> Vec<u8> {
    seal::tagged_aad("user", &[id.as_bytes()])
}
