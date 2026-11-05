mod attachments;
mod backdrop;
mod ck;
pub(crate) mod codec;
mod content;
mod conversations;
mod dbkey;
mod drafts;
mod embeds;
pub mod expiry;
mod gallery;
mod group_meta;
mod history;
mod items;
mod local;
mod media;
mod media_commit;
mod media_rows;
mod media_watch;
pub(crate) mod messages;
pub(crate) mod meta;
pub mod migrations;
mod orphan;
mod people;
mod pins;
pub(crate) mod project;
mod purge;
pub(crate) mod seal;

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use hmac::{Hmac, Mac};
use rusqlite::Connection;
use sha2::Sha256;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::contracts::{
    ChatError, ChatResult, ClientId, ConvId, DeviceId, Events, KeyStore, MessageClass, Millis, Seq,
    SystemEvent, UserId,
};

pub use crate::contracts::{UploadObject, UploadState};
pub use expiry::ExpiryTimer;
pub use media_rows::{MediaUpload, NewUpload, Reserved};
pub use orphan::{ORPHANS_KEPT, remove_database, remove_orphans};
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

#[derive(Clone, PartialEq, Eq)]
pub enum StoredContent {
    Open(Vec<u8>),
    Sealed(Vec<u8>),
    System(SystemEvent),
    Deleted,
}

impl fmt::Debug for StoredContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(bytes) => f.debug_tuple("Open").field(&Hidden(bytes.len())).finish(),
            Self::Sealed(bytes) => f.debug_tuple("Sealed").field(&Hidden(bytes.len())).finish(),
            Self::System(event) => f.debug_tuple("System").field(event).finish(),
            Self::Deleted => f.write_str("Deleted"),
        }
    }
}

pub(crate) struct Hidden(pub(crate) usize);

impl fmt::Debug for Hidden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} bytes>", self.0)
    }
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
    uploads_watch: RwLock<Option<Arc<dyn Events>>>,
    committed: Mutex<HashSet<Uuid>>,
    path: Option<PathBuf>,
}

impl Store {
    pub fn open(path: &Path, master: &[u8; 32]) -> ChatResult<Self> {
        dbkey::owner_only(path)?;
        Self::with_connection(Connection::open(path)?, master, Some(path))
    }

    pub fn open_in_memory(master: &[u8; 32]) -> ChatResult<Self> {
        Self::with_connection(Connection::open_in_memory()?, master, None)
    }

    pub fn open_keyed(path: &Path, keys: &dyn KeyStore) -> ChatResult<Self> {
        orphan::set_strays_aside(path)?;
        let (master, fresh) = dbkey::load_or_create(keys)?;
        if fresh {
            orphan::set_aside(path)?;
        }
        match Self::open(path, &master) {
            Err(ChatError::Crypto) => {
                orphan::set_aside(path)?;
                Self::open(path, &master)
            }
            opened => opened,
        }
    }

    fn with_connection(
        conn: Connection,
        master: &[u8; 32],
        path: Option<&Path>,
    ) -> ChatResult<Self> {
        migrations::prepare(&conn)?;
        migrations::migrate(&conn)?;
        let keys = StoreKeys::derive(master)?;
        dbkey::verify(&conn, &keys)?;
        Ok(Self {
            conn: Mutex::new(conn),
            keys,
            compacted_at: AtomicI64::new(Millis::MIN),
            uploads_watch: RwLock::new(None),
            committed: Mutex::new(HashSet::new()),
            path: path.map(Path::to_path_buf),
        })
    }

    pub fn keys(&self) -> &StoreKeys {
        &self.keys
    }

    pub fn conn(&self) -> ChatResult<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| ChatError::Store)
    }
}
