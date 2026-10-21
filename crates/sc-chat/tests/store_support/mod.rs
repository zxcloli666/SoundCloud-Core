#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use sc_chat::sc_chat_wire::msg::encode_padded;
use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::store::{Store, StoredContent, StoredMessage};
use sc_chat::{
    ChatResult, Clock, ConvId, ConversationKind, ConversationSummary, CoreEvent, Events, KeyName,
    KeyStorage, KeyStore, MemberRole, MemberState, MessageClass, Millis, NotifyLevel, Seq,
    Zeroizing, async_trait,
};
use uuid::Uuid;

pub const ME: &str = "100";
pub const PEER: &str = "200";

pub fn store() -> Store {
    let store = Store::open_in_memory(&[9; 32]).unwrap();
    store.set_me(ME).unwrap();
    store
}

pub fn conv() -> ConvId {
    Uuid::new_v4()
}

pub fn envelope(body: Body) -> proto::Envelope {
    proto::Envelope {
        v: 1,
        body: Some(body),
        ..Default::default()
    }
}

pub fn text(text: &str) -> proto::Text {
    proto::Text {
        text: text.into(),
        spans: Vec::new(),
    }
}

pub fn track(title: &str) -> proto::Embed {
    proto::Embed {
        kind: proto::EmbedKind::Track as i32,
        urn: format!("soundcloud:tracks:{}", title.len()),
        permalink: "https://soundcloud.com/a/b".into(),
        title: title.into(),
        artist: "Artist".into(),
        artwork_url: "https://i1.sndcdn.com/artworks-x-t500x500.jpg".into(),
        duration_ms: 180_000,
        ..Default::default()
    }
}

pub fn row(
    conversation_id: ConvId,
    seq: Seq,
    sender: &str,
    class: MessageClass,
    target_seq: Option<Seq>,
    content: StoredContent,
) -> StoredMessage {
    StoredMessage {
        conversation_id,
        seq,
        client_id: Uuid::new_v4(),
        sender: sender.into(),
        sender_device: None,
        class,
        epoch: 0,
        target_seq,
        ts: seq * 1_000,
        expires_at: None,
        local_only: false,
        content,
    }
}

pub fn open(envelope: &proto::Envelope) -> StoredContent {
    StoredContent::Open(encode_padded(envelope))
}

pub fn post(conversation_id: ConvId, seq: Seq, sender: &str, body: &str) -> StoredMessage {
    row(
        conversation_id,
        seq,
        sender,
        MessageClass::Post,
        None,
        open(&envelope(Body::Text(text(body)))),
    )
}

pub fn post_with_track(
    conversation_id: ConvId,
    seq: Seq,
    body: &str,
    title: &str,
) -> StoredMessage {
    let mut envelope = envelope(Body::Text(text(body)));
    envelope.embeds = vec![track(title)];
    row(
        conversation_id,
        seq,
        PEER,
        MessageClass::Post,
        None,
        open(&envelope),
    )
}

pub fn edit(
    conversation_id: ConvId,
    seq: Seq,
    sender: &str,
    target: Seq,
    body: &str,
) -> StoredMessage {
    row(
        conversation_id,
        seq,
        sender,
        MessageClass::Edit,
        Some(target),
        open(&envelope(Body::Edit(proto::Edit {
            target: target as u64,
            text: Some(text(body)),
        }))),
    )
}

pub fn react(
    conversation_id: ConvId,
    seq: Seq,
    sender: &str,
    target: Seq,
    emoji: &str,
    on: bool,
) -> StoredMessage {
    row(
        conversation_id,
        seq,
        sender,
        MessageClass::React,
        Some(target),
        open(&envelope(Body::Reaction(proto::Reaction {
            target: target as u64,
            emoji: emoji.into(),
            on,
        }))),
    )
}

pub fn summary(id: ConvId, kind: ConversationKind) -> ConversationSummary {
    ConversationSummary {
        id,
        kind,
        title: Some("Group".into()),
        avatar_url: None,
        peer: None,
        role: MemberRole::Member,
        state: MemberState::Active,
        accepted: true,
        member_count: 2,
        last_seq: 0,
        last_at: 0,
        last_read_seq: 0,
        unread: 0,
        mentions: 0,
        preview: None,
        muted_until: None,
        notify: NotifyLevel::All,
        pinned_at: None,
        archived: false,
        ttl_s: None,
        room_code: None,
        history_locked: false,
        has_draft: false,
    }
}

pub fn count(store: &Store, sql: &str, conversation_id: ConvId) -> i64 {
    store
        .conn()
        .unwrap()
        .query_row(sql, [conversation_id.as_bytes()], |row| row.get(0))
        .unwrap()
}

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sc-chat-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
pub struct MemoryKeys(Mutex<HashMap<KeyName, Vec<u8>>>);

impl KeyStore for MemoryKeys {
    fn storage(&self) -> KeyStorage {
        KeyStorage::Os
    }

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&name)
            .cloned()
            .map(Zeroizing::new))
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        self.0.lock().unwrap().insert(name, secret.to_vec());
        Ok(())
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.0.lock().unwrap().remove(&name);
        Ok(())
    }
}

#[derive(Default)]
pub struct ManualClock(pub AtomicI64);

impl ManualClock {
    pub fn set(&self, now: Millis) {
        self.0.store(now, Ordering::Relaxed);
    }
}

#[async_trait]
impl Clock for ManualClock {
    fn now_ms(&self) -> Millis {
        self.0.load(Ordering::Relaxed)
    }

    async fn sleep(&self, ms: u64) {
        self.0.fetch_add(ms as i64, Ordering::Relaxed);
        tokio::task::yield_now().await;
    }
}

#[derive(Default)]
pub struct Recorded(pub Mutex<Vec<CoreEvent>>);

impl Events for Recorded {
    fn emit(&self, event: CoreEvent) {
        self.0.lock().unwrap().push(event);
    }
}
