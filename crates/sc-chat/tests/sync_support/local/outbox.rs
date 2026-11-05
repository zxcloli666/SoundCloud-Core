use sc_chat::outbox::OutboxItem;
use sc_chat::store::{StoredContent, StoredMessage};
use sc_chat::{ClientId, ConvId, Message, Millis, Seq};

use super::MemoryLocal;

impl MemoryLocal {
    pub(super) fn due(&self, now: Millis, limit: usize) -> Vec<OutboxItem> {
        let data = self.data.lock().unwrap();
        data.outbox
            .iter()
            .filter(|(item, failed)| !failed && item.next_at <= now)
            .take(limit)
            .map(|(item, _)| item.clone())
            .collect()
    }

    pub(super) fn sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) {
        let mut data = self.data.lock().unwrap();
        let Some(at) = data
            .outbox
            .iter()
            .position(|(item, _)| item.client_id == client_id)
        else {
            return;
        };
        let (item, _) = data.outbox.remove(at);
        let message = StoredMessage {
            conversation_id: item.conversation_id,
            seq,
            client_id,
            sender: self.me.clone(),
            sender_device: None,
            class: item.class,
            epoch: 0,
            target_seq: item.target_seq,
            ts,
            expires_at,
            local_only: false,
            content: StoredContent::Open(item.envelope),
        };
        data.messages
            .entry((message.conversation_id, seq))
            .or_insert(message);
    }

    pub(super) fn failed(&self, client_id: ClientId, retry_at: Option<Millis>) {
        let mut data = self.data.lock().unwrap();
        for (item, failed) in data.outbox.iter_mut() {
            if item.client_id == client_id {
                item.attempts += 1;
                match retry_at {
                    Some(at) => item.next_at = at,
                    None => *failed = true,
                }
            }
        }
    }

    pub(super) fn pending_of(&self, id: ConvId) -> Vec<Message> {
        let data = self.data.lock().unwrap();
        data.outbox
            .iter()
            .filter(|(item, _)| item.conversation_id == id)
            .map(|(item, failed)| self.pending(item, *failed))
            .collect()
    }
}
