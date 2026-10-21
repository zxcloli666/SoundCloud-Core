mod ck;
pub(crate) mod codec;
mod content;
mod conversations;
mod dbkey;
mod drafts;
mod embeds;
pub mod expiry;
mod history;
mod items;
mod local;
pub(crate) mod messages;
pub(crate) mod meta;
pub mod migrations;
mod people;
pub(crate) mod project;
mod purge;
pub(crate) mod seal;

use std::path::Path;
use std::sync::atomic::AtomicI64;
use std::sync::{Mutex, MutexGuard};

use hmac::{Hmac, Mac};
use rusqlite::Connection;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::contracts::{
    ChatError, ChatResult, ClientId, ConvId, DeviceId, KeyStore, MessageClass, Millis, Seq,
    SystemEvent, UserId,
};

pub use expiry::ExpiryTimer;
pub use purge::COMPACTION_GAP_MS;

const BODY_LABEL: &[u8] = b"chat-store body";
const SEARCH_LABEL: &[u8] = b"chat-store fts";

pub struct StoreKeys {
    pub body: Zeroizing<[u8; 32]>,
    pub search: Zeroizing<[u8; 32]>,
}

impl StoreKeys {
    pub fn derive(master: &[u8; 32]) -> ChatResult<Self> {
        Ok(Self {
            body: subkey(master, BODY_LABEL)?,
            search: subkey(master, SEARCH_LABEL)?,
        })
    }
}

fn subkey(master: &[u8; 32], label: &[u8]) -> ChatResult<Zeroizing<[u8; 32]>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(master).map_err(|_| ChatError::Crypto)?;
    mac.update(label);
    Ok(Zeroizing::new(mac.finalize().into_bytes().into()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncState {
    pub pos: u64,
    pub inc: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredContent {
    Open(Vec<u8>),
    Sealed(Vec<u8>),
    System(SystemEvent),
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub conversation_id: ConvId,
    pub seq: Seq,
    pub client_id: ClientId,
    pub sender: UserId,
    pub sender_device: Option<DeviceId>,
    pub class: MessageClass,
    pub epoch: u32,
    pub target_seq: Option<Seq>,
    pub ts: Millis,
    pub expires_at: Option<Millis>,
    pub local_only: bool,
    pub content: StoredContent,
}

pub struct Store {
    conn: Mutex<Connection>,
    keys: StoreKeys,
    compacted_at: AtomicI64,
}

impl Store {
    pub fn open(path: &Path, master: &[u8; 32]) -> ChatResult<Self> {
        dbkey::owner_only(path)?;
        Self::with_connection(Connection::open(path)?, master)
    }

    pub fn open_in_memory(master: &[u8; 32]) -> ChatResult<Self> {
        Self::with_connection(Connection::open_in_memory()?, master)
    }

    pub fn open_keyed(path: &Path, keys: &dyn KeyStore) -> ChatResult<Self> {
        let (master, fresh) = dbkey::load_or_create(keys)?;
        if fresh {
            dbkey::remove_files(path)?;
        }
        match Self::open(path, &master) {
            Err(ChatError::Crypto) => {
                dbkey::remove_files(path)?;
                Self::open(path, &master)
            }
            opened => opened,
        }
    }

    fn with_connection(conn: Connection, master: &[u8; 32]) -> ChatResult<Self> {
        migrations::prepare(&conn)?;
        migrations::migrate(&conn)?;
        let keys = StoreKeys::derive(master)?;
        dbkey::verify(&conn, &keys)?;
        Ok(Self {
            conn: Mutex::new(conn),
            keys,
            compacted_at: AtomicI64::new(Millis::MIN),
        })
    }

    pub fn keys(&self) -> &StoreKeys {
        &self.keys
    }

    pub fn conn(&self) -> ChatResult<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| ChatError::Store)
    }
}
