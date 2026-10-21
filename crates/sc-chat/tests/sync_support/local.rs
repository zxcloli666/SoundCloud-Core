use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use sc_chat::crypto::Ck;
use sc_chat::embeds::wire::from_proto;
use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::{Envelope, envelope::Body};
use sc_chat::store::{StoredContent, StoredMessage, SyncState};
use sc_chat::sync::Local;
use sc_chat::{
    ChatResult, ClientId, ConvId, ConversationSummary, DeliveryState, GoneMessage, HistoryPage,
    HistoryQuery, Message, MessageBody, MessageClass, Millis, PeerMark, RichText, Seq, UserId,
    UserRef,
};

#[derive(Default)]
struct Data {
    sync: SyncState,
    conversations: BTreeMap<ConvId, ConversationSummary>,
    messages: BTreeMap<(ConvId, Seq), StoredMessage>,
    marks: HashMap<ConvId, Vec<PeerMark>>,
    users: HashMap<UserId, UserRef>,
    keys: HashMap<(ConvId, u32), [u8; 32]>,
    outbox: Vec<(OutboxItem, bool)>,
    kept_above: Vec<(ConvId, Seq)>,
}

pub struct MemoryLocal {
    me: String,
    data: Mutex<Data>,
}

impl MemoryLocal {
    pub fn new(me: &str) -> Self {
        Self {
            me: me.to_owned(),
            data: Mutex::new(Data::default()),
        }
    }

    pub fn stored(&self, id: ConvId) -> Vec<StoredMessage> {
        let data = self.data.lock().unwrap();
        data.messages
            .values()
            .filter(|message| message.conversation_id == id)
            .cloned()
            .collect()
    }

    pub fn summary(&self, id: ConvId) -> Option<ConversationSummary> {
        self.data.lock().unwrap().conversations.get(&id).cloned()
    }

    pub fn has_key(&self, id: ConvId, epoch: u32) -> bool {
        self.data.lock().unwrap().keys.contains_key(&(id, epoch))
    }

    pub fn add_key(&self, id: ConvId, epoch: u32, bytes: [u8; 32]) {
        self.data.lock().unwrap().keys.insert((id, epoch), bytes);
    }

    pub fn queue(&self, item: OutboxItem) {
        self.data.lock().unwrap().outbox.push((item, false));
    }

    pub fn outbox(&self) -> Vec<(OutboxItem, bool)> {
        self.data.lock().unwrap().outbox.clone()
    }

    pub fn kept_above(&self) -> Vec<(ConvId, Seq)> {
        self.data.lock().unwrap().kept_above.clone()
    }

    pub fn marks_of(&self, id: ConvId) -> Vec<PeerMark> {
        self.data
            .lock()
            .unwrap()
            .marks
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    fn render(&self, stored: &StoredMessage) -> Message {
        let mut embeds = Vec::new();
        let body = match &stored.content {
            StoredContent::Open(bytes) => match Envelope::decode(bytes.as_slice()) {
                Ok(envelope) => {
                    embeds = envelope.embeds.iter().filter_map(from_proto).collect();
                    match envelope.body {
                        Some(Body::Text(text)) => MessageBody::Text(RichText {
                            text: text.text,
                            spans: Vec::new(),
                        }),
                        _ => MessageBody::Unknown,
                    }
                }
                Err(_) => MessageBody::Unknown,
            },
            StoredContent::Sealed(_) => MessageBody::Locked,
            StoredContent::System(event) => MessageBody::System(event.clone()),
            StoredContent::Deleted => MessageBody::Deleted,
        };
        Message {
            conversation_id: stored.conversation_id,
            seq: Some(stored.seq),
            client_id: stored.client_id,
            sender: stored.sender.clone(),
            sender_device: stored.sender_device,
            mine: stored.sender == self.me,
            ts: stored.ts,
            edited_at: None,
            expires_at: stored.expires_at,
            state: DeliveryState::Sent,
            body,
            reply_to: None,
            mentions: Vec::new(),
            mentions_me: false,
            embeds,
            forwarded_from: None,
            reactions: Vec::new(),
            pinned: false,
            local_only: stored.local_only,
        }
    }

    fn pending(&self, item: &OutboxItem, failed: bool) -> Message {
        Message {
            conversation_id: item.conversation_id,
            seq: None,
            client_id: item.client_id,
            sender: self.me.clone(),
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
        }
    }
}

fn visible(class: MessageClass) -> bool {
    matches!(class, MessageClass::Post | MessageClass::System)
}

impl Local for MemoryLocal {
    fn sync_state(&self) -> ChatResult<SyncState> {
        Ok(self.data.lock().unwrap().sync)
    }

    fn set_sync_state(&self, state: SyncState) -> ChatResult<()> {
        self.data.lock().unwrap().sync = state;
        Ok(())
    }

    fn known(&self, limit: usize) -> ChatResult<Vec<(ConvId, Seq)>> {
        let data = self.data.lock().unwrap();
        let mut rows: Vec<&ConversationSummary> = data.conversations.values().collect();
        rows.sort_by_key(|summary| std::cmp::Reverse(summary.last_at));
        Ok(rows
            .into_iter()
            .take(limit)
            .map(|summary| (summary.id, summary.last_seq))
            .collect())
    }

    fn conversation(&self, id: ConvId) -> ChatResult<Option<ConversationSummary>> {
        Ok(self.summary(id))
    }

    fn put_conversations(&self, items: &[ConversationSummary]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        for item in items {
            data.conversations.insert(item.id, item.clone());
        }
        Ok(())
    }

    fn users(&self, ids: &[UserId]) -> ChatResult<Vec<UserRef>> {
        let data = self.data.lock().unwrap();
        Ok(ids
            .iter()
            .filter_map(|id| data.users.get(id).cloned())
            .collect())
    }

    fn put_users(&self, users: &[UserRef]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        for user in users {
            data.users.insert(user.id.clone(), user.clone());
        }
        Ok(())
    }

    fn put_marks(&self, id: ConvId, marks: &[PeerMark]) -> ChatResult<()> {
        self.data.lock().unwrap().marks.insert(id, marks.to_vec());
        Ok(())
    }

    fn put_messages(&self, items: &[StoredMessage]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        for item in items {
            data.outbox
                .retain(|(queued, _)| queued.client_id != item.client_id);
            data.messages
                .insert((item.conversation_id, item.seq), item.clone());
        }
        Ok(())
    }

    fn messages(&self, id: ConvId, query: &HistoryQuery) -> ChatResult<HistoryPage> {
        let data = self.data.lock().unwrap();
        let mut rows: Vec<&StoredMessage> = data
            .messages
            .values()
            .filter(|message| message.conversation_id == id && visible(message.class))
            .filter(|message| query.after.is_none_or(|after| message.seq > after))
            .filter(|message| query.before.is_none_or(|before| message.seq < before))
            .collect();
        let limit = query.limit.max(1) as usize;
        let more = rows.len() > limit;
        if query.after.is_some() {
            rows.truncate(limit);
        } else {
            let skip = rows.len().saturating_sub(limit);
            rows.drain(..skip);
        }
        let messages = rows.into_iter().map(|row| self.render(row)).collect();
        Ok(HistoryPage { messages, more })
    }

    fn sealed_messages(&self, id: ConvId, epoch: u32) -> ChatResult<Vec<StoredMessage>> {
        Ok(self
            .stored(id)
            .into_iter()
            .filter(|message| message.epoch == epoch)
            .filter(|message| matches!(message.content, StoredContent::Sealed(_)))
            .collect())
    }

    fn put_ck(&self, id: ConvId, epoch: u32, ck: &Ck) -> ChatResult<()> {
        self.add_key(id, epoch, *ck.as_bytes());
        Ok(())
    }

    fn ck(&self, id: ConvId, epoch: u32) -> ChatResult<Option<Ck>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .keys
            .get(&(id, epoch))
            .map(|bytes| Ck::from_bytes(*bytes)))
    }

    fn purge_conversations(&self, ids: &[ConvId]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        for id in ids {
            data.conversations.remove(id);
            data.messages.retain(|(conv, _), _| conv != id);
            data.keys.retain(|(conv, _), _| conv != id);
            data.marks.remove(id);
        }
        Ok(())
    }

    fn purge_messages(&self, gone: &[GoneMessage]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        for message in gone {
            data.messages
                .remove(&(message.conversation_id, message.seq));
        }
        Ok(())
    }

    fn keep_local_above(&self, server_last: &[(ConvId, Seq)]) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        data.kept_above.extend_from_slice(server_last);
        for (id, last) in server_last {
            let above: Vec<(ConvId, Seq)> = data
                .messages
                .keys()
                .filter(|(conv, seq)| conv == id && seq > last)
                .copied()
                .collect();
            for key in above {
                if let Some(mut message) = data.messages.remove(&key) {
                    message.seq -= 1 << 40;
                    message.local_only = true;
                    data.messages.insert((key.0, message.seq), message);
                }
            }
            if let Some(summary) = data.conversations.get_mut(id) {
                summary.last_seq = *last;
            }
        }
        Ok(())
    }

    fn outbox_due(&self, now: Millis, limit: usize) -> ChatResult<Vec<OutboxItem>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .outbox
            .iter()
            .filter(|(item, failed)| !failed && item.next_at <= now)
            .take(limit)
            .map(|(item, _)| item.clone())
            .collect())
    }

    fn outbox_sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        let Some(at) = data
            .outbox
            .iter()
            .position(|(item, _)| item.client_id == client_id)
        else {
            return Ok(());
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
        Ok(())
    }

    fn outbox_failed(&self, client_id: ClientId, retry_at: Option<Millis>) -> ChatResult<()> {
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
        Ok(())
    }

    fn outbox_pending(&self, id: ConvId) -> ChatResult<Vec<Message>> {
        let data = self.data.lock().unwrap();
        Ok(data
            .outbox
            .iter()
            .filter(|(item, _)| item.conversation_id == id)
            .map(|(item, failed)| self.pending(item, *failed))
            .collect())
    }
}
