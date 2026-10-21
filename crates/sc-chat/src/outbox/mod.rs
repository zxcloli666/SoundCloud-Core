mod rows;

use std::sync::Arc;

use rusqlite::params;

use crate::contracts::{
    ChatError, ChatResult, ClientId, ConvId, DeliveryState, Message, MessageBody, MessageClass,
    Millis, Seq,
};
use crate::store::{Store, StoredContent, StoredMessage, messages, meta, project, seal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxItem {
    pub client_id: ClientId,
    pub conversation_id: ConvId,
    pub class: MessageClass,
    pub target_seq: Option<Seq>,
    pub envelope: Vec<u8>,
    pub created_at: Millis,
    pub attempts: u32,
    pub next_at: Millis,
}

pub struct Outbox {
    store: Arc<Store>,
}

impl Outbox {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn push(&self, item: OutboxItem) -> ChatResult<Message> {
        let local_target = item.target_seq.is_some_and(|seq| seq < 0);
        if item.class == MessageClass::System || local_target {
            return Err(ChatError::Invalid);
        }
        let conn = self.store.conn()?;
        let sealed = seal::seal(
            &self.store.keys().body,
            &rows::aad(item.client_id),
            &item.envelope,
        )?;
        conn.execute(
            "INSERT INTO outbox (client_id, conversation_id, class, target_seq, created_at, attempts, next_at, failed, nonce, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9) ON CONFLICT (client_id) DO UPDATE SET next_at = excluded.next_at, failed = 0, nonce = excluded.nonce, body = excluded.body",
            params![
                item.client_id.as_bytes(),
                item.conversation_id.as_bytes(),
                item.class.as_str(),
                item.target_seq,
                item.created_at,
                item.attempts,
                item.next_at,
                sealed.nonce,
                sealed.body,
            ],
        )?;
        let me = meta::me(&conn)?;
        Ok(pending(&item, me.unwrap_or_default(), false))
    }

    pub fn due(&self, now: Millis, limit: usize) -> ChatResult<Vec<OutboxItem>> {
        let conn = self.store.conn()?;
        let found = rows::load(
            &conn,
            self.store.keys(),
            "failed = 0 AND next_at <= ?1 ORDER BY created_at, client_id LIMIT ?2",
            params![now, limit as i64],
        )?;
        Ok(found.into_iter().map(|(item, _)| item).collect())
    }

    pub fn sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) -> ChatResult<()> {
        let mut conn = self.store.conn()?;
        let tx = conn.transaction()?;
        let item = rows::load(
            &tx,
            self.store.keys(),
            "client_id = ?1",
            [client_id.as_bytes()],
        )?
        .pop();
        let Some((item, _)) = item else {
            return Ok(());
        };
        tx.execute(
            "DELETE FROM outbox WHERE client_id = ?1",
            [client_id.as_bytes()],
        )?;
        let me = meta::me(&tx)?;
        if let Some(me) = me
            && !rows::has_message(&tx, item.conversation_id, seq)?
        {
            let stored = StoredMessage {
                conversation_id: item.conversation_id,
                seq,
                client_id,
                sender: me,
                sender_device: None,
                class: item.class,
                epoch: rows::key_epoch(&tx, item.conversation_id)?,
                target_seq: item.target_seq,
                ts,
                expires_at,
                local_only: false,
                content: StoredContent::Open(item.envelope),
            };
            messages::write(&tx, self.store.keys(), &stored)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn failed(&self, client_id: ClientId, retry_at: Option<Millis>) -> ChatResult<()> {
        self.store.conn()?.execute(
            "UPDATE outbox SET attempts = attempts + 1, next_at = coalesce(?2, next_at), failed = (?2 IS NULL) WHERE client_id = ?1",
            params![client_id.as_bytes(), retry_at],
        )?;
        Ok(())
    }

    pub fn retry(&self, client_id: ClientId, now: Millis) -> ChatResult<()> {
        self.store.conn()?.execute(
            "UPDATE outbox SET failed = 0, next_at = ?2 WHERE client_id = ?1",
            params![client_id.as_bytes(), now],
        )?;
        Ok(())
    }

    pub fn discard(&self, client_id: ClientId) -> ChatResult<()> {
        self.store.conn()?.execute(
            "DELETE FROM outbox WHERE client_id = ?1",
            [client_id.as_bytes()],
        )?;
        Ok(())
    }

    pub fn pending(&self, conversation_id: ConvId) -> ChatResult<Vec<Message>> {
        let conn = self.store.conn()?;
        let me = meta::me(&conn)?.unwrap_or_default();
        let found = rows::load(
            &conn,
            self.store.keys(),
            "conversation_id = ?1 AND class = 'post' ORDER BY created_at, client_id",
            [conversation_id.as_bytes()],
        )?;
        Ok(found
            .iter()
            .map(|(item, failed)| pending(item, me.clone(), *failed))
            .collect())
    }
}

fn pending(item: &OutboxItem, sender: String, failed: bool) -> Message {
    let mut message = Message {
        conversation_id: item.conversation_id,
        seq: None,
        client_id: item.client_id,
        sender,
        sender_device: None,
        mine: true,
        ts: item.created_at,
        edited_at: None,
        expires_at: None,
        state: if failed {
            DeliveryState::Failed
        } else {
            DeliveryState::Pending
        },
        body: MessageBody::Unknown,
        reply_to: None,
        mentions: Vec::new(),
        mentions_me: false,
        embeds: Vec::new(),
        forwarded_from: None,
        reactions: Vec::new(),
        pinned: false,
        local_only: false,
    };
    project::fill_body(&mut message, &StoredContent::Open(item.envelope.clone()));
    message
}
