mod outbox;
mod render;
mod store;

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use sc_chat::outbox::OutboxItem;
use sc_chat::store::{StoredMessage, SyncState};
use sc_chat::{ConvId, ConversationMeta, ConversationSummary, PeerMark, Seq, UserId, UserRef};

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
    group_meta: HashMap<ConvId, ConversationMeta>,
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

    pub fn group_meta(&self, id: ConvId) -> Option<ConversationMeta> {
        self.data.lock().unwrap().group_meta.get(&id).cloned()
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
}
