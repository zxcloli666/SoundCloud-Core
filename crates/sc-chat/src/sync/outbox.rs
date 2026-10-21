use sc_chat_wire::prost::Message as _;
use sc_chat_wire::proto::Envelope;

use super::local::Local;
use super::{SyncEngine, fatal, jitter, summary};
use crate::api::wire::{SendAnswer, SendMessage};
use crate::contracts::{
    ChatError, ChatResult, ClientId, ConvId, CoreEvent, HistoryQuery, Message, MessagesEvent,
    MessagesOrigin, Seq,
};
use crate::crypto::MessageAt;
use crate::outbox::OutboxItem;

pub const OUTBOX_BATCH: usize = 16;
pub const OUTBOX_ROUNDS: usize = 8;
pub const SEND_ATTEMPTS_MAX: u32 = 10;
pub const SEND_RETRY_START_MS: u64 = 2_000;
pub const SEND_RETRY_MAX_MS: u64 = 60_000;
pub const SEND_PAUSED_RETRY_MS: u64 = 60_000;

impl<L: Local> SyncEngine<L> {
    pub async fn flush_outbox(&self) -> ChatResult<usize> {
        let mut sent = 0;
        for _ in 0..OUTBOX_ROUNDS {
            let due = self.store.outbox_due(self.now(), OUTBOX_BATCH)?;
            for item in &due {
                match self.send_item(item).await {
                    Ok(answer) => {
                        self.store.outbox_sent(
                            item.client_id,
                            answer.seq,
                            answer.ts,
                            answer.expires_at,
                        )?;
                        self.after_send(item, answer.seq)?;
                        sent += 1;
                    }
                    Err(error) if fatal(&error) => return Err(error),
                    Err(error) => self.send_failed(item, &error)?,
                }
            }
            if due.len() < OUTBOX_BATCH {
                break;
            }
        }
        Ok(sent)
    }

    async fn send_item(&self, item: &OutboxItem) -> ChatResult<SendAnswer> {
        match self.send_once(item, false).await {
            Err(ChatError::EpochStale) => self.send_once(item, true).await,
            other => other,
        }
    }

    async fn send_once(&self, item: &OutboxItem, stale: bool) -> ChatResult<SendAnswer> {
        let identity = self.api.identity().ok_or(ChatError::NotReady)?;
        let envelope =
            Envelope::decode(item.envelope.as_slice()).map_err(|_| ChatError::Invalid)?;
        let (epoch, ck) = self.send_key(item.conversation_id, stale).await?;
        let at = MessageAt {
            conversation_id: item.conversation_id,
            epoch,
            sender_device: identity.device_id,
            client_id: item.client_id,
            class: item.class,
        };
        let body = self.cipher.seal_message(&ck, &at, &envelope)?;
        let message = SendMessage {
            client_id: item.client_id,
            class: item.class.as_str().to_owned(),
            epoch: i32::try_from(epoch).map_err(|_| ChatError::Invalid)?,
            target_seq: item.target_seq,
            body,
        };
        self.api.send(item.conversation_id, &message).await
    }

    fn after_send(&self, item: &OutboxItem, seq: Seq) -> ChatResult<()> {
        let conversation_id = item.conversation_id;
        let mut messages = self.message_at(conversation_id, seq)?;
        if let Some(mut summary) = self.store.conversation(conversation_id)? {
            let contiguous = summary.last_seq;
            summary::absorb(&mut summary, &messages);
            if seq > contiguous + 1 {
                summary.last_seq = contiguous;
            }
            self.save(std::slice::from_ref(&summary))?;
            self.emit_summaries(vec![summary]);
        }
        if let Some(target) = item.target_seq {
            messages.extend(self.message_at(conversation_id, target)?);
        }
        self.emit_local(conversation_id, messages);
        Ok(())
    }

    fn message_at(&self, conversation_id: ConvId, seq: Seq) -> ChatResult<Vec<Message>> {
        let query = HistoryQuery {
            before: None,
            after: Some(seq.saturating_sub(1)),
            limit: 1,
        };
        Ok(self
            .store
            .messages(conversation_id, &query)?
            .messages
            .into_iter()
            .filter(|message| message.seq == Some(seq))
            .collect())
    }

    fn send_failed(&self, item: &OutboxItem, error: &ChatError) -> ChatResult<()> {
        let attempt = item.attempts.saturating_add(1);
        let wait = match error {
            ChatError::Offline | ChatError::Internal | ChatError::Protocol
                if attempt < SEND_ATTEMPTS_MAX =>
            {
                Some(jitter::around(jitter::doubling(
                    attempt,
                    SEND_RETRY_START_MS,
                    SEND_RETRY_MAX_MS,
                )))
            }
            ChatError::Busy { retry_ms } | ChatError::SlowMode { retry_ms }
                if attempt < SEND_ATTEMPTS_MAX =>
            {
                Some((*retry_ms).clamp(SEND_RETRY_START_MS, SEND_RETRY_MAX_MS))
            }
            ChatError::SendPaused => Some(SEND_PAUSED_RETRY_MS),
            _ => None,
        };
        let retry_at = wait.map(|ms| {
            self.now()
                .saturating_add(i64::try_from(ms).unwrap_or(i64::MAX))
        });
        self.store.outbox_failed(item.client_id, retry_at)?;
        if retry_at.is_none() {
            self.emit_failed(item.conversation_id, item.client_id)?;
        }
        Ok(())
    }

    fn emit_failed(&self, conversation_id: ConvId, client_id: ClientId) -> ChatResult<()> {
        let failed: Vec<Message> = self
            .store
            .outbox_pending(conversation_id)?
            .into_iter()
            .filter(|message| message.client_id == client_id)
            .collect();
        self.emit_local(conversation_id, failed);
        Ok(())
    }

    fn emit_local(&self, conversation_id: ConvId, messages: Vec<Message>) {
        if messages.is_empty() {
            return;
        }
        self.events.emit(CoreEvent::Messages(MessagesEvent {
            conversation_id,
            origin: MessagesOrigin::Local,
            messages,
        }));
    }
}
