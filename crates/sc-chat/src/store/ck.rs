use rusqlite::{OptionalExtension, params};

use super::{Store, seal};
use crate::contracts::{ChatError, ChatResult, ConvId};
use crate::crypto::Ck;

impl Store {
    pub fn put_ck(&self, id: ConvId, epoch: u32, ck: &Ck) -> ChatResult<()> {
        let sealed = seal::seal(&self.keys.body, &aad(id, epoch), ck.as_bytes())?;
        self.conn()?.execute(
            "INSERT INTO conversation_keys (conversation_id, epoch, nonce, ck) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (conversation_id, epoch) DO UPDATE SET nonce = excluded.nonce, ck = excluded.ck",
            params![id.as_bytes(), epoch, sealed.nonce, sealed.body],
        )?;
        Ok(())
    }

    pub fn ck(&self, id: ConvId, epoch: u32) -> ChatResult<Option<Ck>> {
        let found: Option<(Vec<u8>, Vec<u8>)> = self
            .conn()?
            .query_row(
                "SELECT nonce, ck FROM conversation_keys WHERE conversation_id = ?1 AND epoch = ?2",
                params![id.as_bytes(), epoch],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((nonce, body)) = found else {
            return Ok(None);
        };
        let plain = seal::open(&self.keys.body, &aad(id, epoch), &nonce, &body)?;
        let bytes = <[u8; 32]>::try_from(plain.as_slice()).map_err(|_| ChatError::Crypto)?;
        Ok(Some(Ck::from_bytes(bytes)))
    }
}

fn aad(id: ConvId, epoch: u32) -> Vec<u8> {
    seal::tagged_aad("ck", &[id.as_bytes(), &epoch.to_be_bytes()])
}
