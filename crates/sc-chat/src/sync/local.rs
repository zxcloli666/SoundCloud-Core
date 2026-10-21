use std::sync::Arc;

use crate::contracts::{
    ChatResult, ClientId, ConvId, ConversationSummary, GoneMessage, HistoryPage, HistoryQuery,
    Message, Millis, PeerMark, Seq, UserId, UserRef,
};
use crate::crypto::Ck;
use crate::outbox::{Outbox, OutboxItem};
use crate::store::{Store, StoredMessage, SyncState};

pub trait Local: Send + Sync {
    fn sync_state(&self) -> ChatResult<SyncState>;
    fn set_sync_state(&self, state: SyncState) -> ChatResult<()>;
    fn known(&self, limit: usize) -> ChatResult<Vec<(ConvId, Seq)>>;
    fn conversation(&self, id: ConvId) -> ChatResult<Option<ConversationSummary>>;
    fn put_conversations(&self, items: &[ConversationSummary]) -> ChatResult<()>;
    fn users(&self, ids: &[UserId]) -> ChatResult<Vec<UserRef>>;
    fn put_users(&self, users: &[UserRef]) -> ChatResult<()>;
    fn put_marks(&self, id: ConvId, marks: &[PeerMark]) -> ChatResult<()>;
    fn put_messages(&self, items: &[StoredMessage]) -> ChatResult<()>;
    fn messages(&self, id: ConvId, query: &HistoryQuery) -> ChatResult<HistoryPage>;
    fn sealed_messages(&self, id: ConvId, epoch: u32) -> ChatResult<Vec<StoredMessage>>;
    fn put_ck(&self, id: ConvId, epoch: u32, ck: &Ck) -> ChatResult<()>;
    fn ck(&self, id: ConvId, epoch: u32) -> ChatResult<Option<Ck>>;
    fn purge_conversations(&self, ids: &[ConvId]) -> ChatResult<()>;
    fn purge_messages(&self, gone: &[GoneMessage]) -> ChatResult<()>;
    fn keep_local_above(&self, server_last: &[(ConvId, Seq)]) -> ChatResult<()>;
    fn outbox_due(&self, now: Millis, limit: usize) -> ChatResult<Vec<OutboxItem>>;
    fn outbox_sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) -> ChatResult<()>;
    fn outbox_failed(&self, client_id: ClientId, retry_at: Option<Millis>) -> ChatResult<()>;
    fn outbox_pending(&self, id: ConvId) -> ChatResult<Vec<Message>>;
}

pub struct LocalStore {
    store: Arc<Store>,
    outbox: Outbox,
}

impl LocalStore {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            outbox: Outbox::new(store.clone()),
            store,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
}

impl Local for LocalStore {
    fn sync_state(&self) -> ChatResult<SyncState> {
        self.store.sync_state()
    }

    fn set_sync_state(&self, state: SyncState) -> ChatResult<()> {
        self.store.set_sync_state(state)
    }

    fn known(&self, limit: usize) -> ChatResult<Vec<(ConvId, Seq)>> {
        self.store.known(limit)
    }

    fn conversation(&self, id: ConvId) -> ChatResult<Option<ConversationSummary>> {
        self.store.conversation(id)
    }

    fn put_conversations(&self, items: &[ConversationSummary]) -> ChatResult<()> {
        self.store.put_conversations(items)
    }

    fn users(&self, ids: &[UserId]) -> ChatResult<Vec<UserRef>> {
        self.store.users(ids)
    }

    fn put_users(&self, users: &[UserRef]) -> ChatResult<()> {
        self.store.put_users(users)
    }

    fn put_marks(&self, id: ConvId, marks: &[PeerMark]) -> ChatResult<()> {
        self.store.put_marks(id, marks)
    }

    fn put_messages(&self, items: &[StoredMessage]) -> ChatResult<()> {
        self.store.put_messages(items)
    }

    fn messages(&self, id: ConvId, query: &HistoryQuery) -> ChatResult<HistoryPage> {
        self.store.messages(id, query)
    }

    fn sealed_messages(&self, id: ConvId, epoch: u32) -> ChatResult<Vec<StoredMessage>> {
        self.store.sealed_messages(id, epoch)
    }

    fn put_ck(&self, id: ConvId, epoch: u32, ck: &Ck) -> ChatResult<()> {
        self.store.put_ck(id, epoch, ck)
    }

    fn ck(&self, id: ConvId, epoch: u32) -> ChatResult<Option<Ck>> {
        self.store.ck(id, epoch)
    }

    fn purge_conversations(&self, ids: &[ConvId]) -> ChatResult<()> {
        self.store.purge_conversations(ids)
    }

    fn purge_messages(&self, gone: &[GoneMessage]) -> ChatResult<()> {
        self.store.purge_messages(gone)
    }

    fn keep_local_above(&self, server_last: &[(ConvId, Seq)]) -> ChatResult<()> {
        self.store.keep_local_above(server_last)
    }

    fn outbox_due(&self, now: Millis, limit: usize) -> ChatResult<Vec<OutboxItem>> {
        self.outbox.due(now, limit)
    }

    fn outbox_sent(
        &self,
        client_id: ClientId,
        seq: Seq,
        ts: Millis,
        expires_at: Option<Millis>,
    ) -> ChatResult<()> {
        self.outbox.sent(client_id, seq, ts, expires_at)
    }

    fn outbox_failed(&self, client_id: ClientId, retry_at: Option<Millis>) -> ChatResult<()> {
        self.outbox.failed(client_id, retry_at)
    }

    fn outbox_pending(&self, id: ConvId) -> ChatResult<Vec<Message>> {
        self.outbox.pending(id)
    }
}
