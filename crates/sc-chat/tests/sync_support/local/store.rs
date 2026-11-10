use sc_chat::crypto::Ck;
use sc_chat::outbox::OutboxItem;
use sc_chat::store::{StoredContent, StoredMessage, SyncState};
use sc_chat::sync::Local;
use sc_chat::{
    ChatError, ChatResult, ClientId, ConvId, ConversationKind, ConversationMeta,
    ConversationSummary, GoneMessage, HistoryPage, HistoryQuery, MemberState, Message, Millis,
    PeerMark, Seq, UserId, UserRef,
};

use super::MemoryLocal;
use super::render::visible;

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

    fn put_group_meta(&self, id: ConvId, meta: &ConversationMeta) -> ChatResult<()> {
        self.data
            .lock()
            .unwrap()
            .group_meta
            .insert(id, meta.clone());
        Ok(())
    }

    fn put_room_meta(&self, id: ConvId, meta: Option<&ConversationMeta>) -> ChatResult<()> {
        let mut data = self.data.lock().unwrap();
        match meta {
            Some(meta) => data.group_meta.insert(id, meta.clone()),
            None => data.group_meta.remove(&id),
        };
        Ok(())
    }

    fn quiet_rooms(&self, limit: usize) -> ChatResult<Vec<ConvId>> {
        let data = self.data.lock().unwrap();
        let mut rooms: Vec<&ConversationSummary> = data
            .conversations
            .values()
            .filter(|summary| {
                summary.kind == ConversationKind::Room
                    && summary.state == MemberState::Active
                    && !data.group_meta.contains_key(&summary.id)
            })
            .collect();
        rooms.sort_by_key(|summary| std::cmp::Reverse(summary.last_at));
        Ok(rooms
            .into_iter()
            .take(limit)
            .map(|summary| summary.id)
            .collect())
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

    fn marks(&self, id: ConvId) -> ChatResult<Vec<PeerMark>> {
        Ok(self
            .data
            .lock()
            .unwrap()
            .marks
            .get(&id)
            .cloned()
            .unwrap_or_default())
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
        if self
            .data
            .lock()
            .unwrap()
            .refused_keys
            .contains(&(id, epoch))
        {
            return Err(ChatError::Store);
        }
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
        Ok(self.due(now, limit))
    }

    fn outbox_sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) -> ChatResult<()> {
        self.sent(client_id, seq, ts, expires_at);
        Ok(())
    }

    fn outbox_failed(&self, client_id: ClientId, retry_at: Option<Millis>) -> ChatResult<()> {
        self.failed(client_id, retry_at);
        Ok(())
    }

    fn outbox_pending(&self, id: ConvId) -> ChatResult<Vec<Message>> {
        Ok(self.pending_of(id))
    }
}
