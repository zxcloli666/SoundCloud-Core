use rusqlite::{OptionalExtension, params};

use super::codec::{from_json, to_json};
use super::{Store, seal};
use crate::contracts::{ChatResult, ConvId, RichText};

impl Store {
    pub fn save_draft(&self, id: ConvId, draft: Option<&RichText>) -> ChatResult<()> {
        let conn = self.conn()?;
        let Some(draft) = draft.filter(|draft| !draft.text.trim().is_empty()) else {
            conn.execute(
                "DELETE FROM drafts WHERE conversation_id = ?1",
                [id.as_bytes()],
            )?;
            return Ok(());
        };
        let sealed = seal::seal(&self.keys.body, &aad(id), &to_json(draft)?)?;
        conn.execute(
            "INSERT INTO drafts (conversation_id, updated_at, nonce, body) VALUES (?1, CAST(unixepoch('subsec') * 1000 AS INTEGER), ?2, ?3) ON CONFLICT (conversation_id) DO UPDATE SET updated_at = excluded.updated_at, nonce = excluded.nonce, body = excluded.body",
            params![id.as_bytes(), sealed.nonce, sealed.body],
        )?;
        Ok(())
    }

    pub fn draft(&self, id: ConvId) -> ChatResult<Option<RichText>> {
        let found: Option<(Vec<u8>, Vec<u8>)> = self
            .conn()?
            .query_row(
                "SELECT nonce, body FROM drafts WHERE conversation_id = ?1",
                [id.as_bytes()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((nonce, body)) = found else {
            return Ok(None);
        };
        let plain = seal::open(&self.keys.body, &aad(id), &nonce, &body)?;
        Ok(Some(from_json(&plain)?))
    }
}

fn aad(id: ConvId) -> Vec<u8> {
    seal::tagged_aad("draft", &[id.as_bytes()])
}
