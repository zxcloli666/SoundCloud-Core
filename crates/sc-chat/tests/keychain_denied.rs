mod store_support;

use std::sync::atomic::{AtomicBool, Ordering};

use sc_chat::crypto::DeviceKeys;
use sc_chat::store::Store;
use sc_chat::{ChatError, ChatPhase, ChatResult, KeyName, KeyStorage, KeyStore, Zeroizing};
use store_support::{MemoryKeys, PEER, TempDir, conv, post};

#[derive(Default)]
struct Guarded {
    keys: MemoryKeys,
    denied: AtomicBool,
}

impl Guarded {
    fn deny(&self, denied: bool) {
        self.denied.store(denied, Ordering::SeqCst);
    }

    fn ask(&self) -> ChatResult<()> {
        match self.denied.load(Ordering::SeqCst) {
            true => Err(ChatError::KeychainDenied),
            false => Ok(()),
        }
    }
}

impl KeyStore for Guarded {
    fn storage(&self) -> KeyStorage {
        KeyStorage::Os
    }

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>> {
        self.ask()?;
        self.keys.load(name)
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        self.ask()?;
        self.keys.save(name, secret)
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.ask()?;
        self.keys.delete(name)
    }
}

fn messages(store: &Store) -> i64 {
    store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_denied_keychain_reaches_the_caller_unchanged_and_keeps_the_history() {
    let dir = TempDir::new("denied");
    let path = dir.file("chat.db");
    let keys = Guarded::default();
    Store::open_keyed(&path, &keys)
        .unwrap()
        .put_messages(&[post(conv(), 1, PEER, "останется")])
        .unwrap();
    let (device_id, _) = DeviceKeys::load_or_create(&keys).unwrap();
    let size = std::fs::metadata(&path).unwrap().len();

    keys.deny(true);
    assert_eq!(
        Store::open_keyed(&path, &keys).err(),
        Some(ChatError::KeychainDenied)
    );
    assert_eq!(
        DeviceKeys::load_or_create(&keys).err(),
        Some(ChatError::KeychainDenied)
    );
    assert_eq!(std::fs::metadata(&path).unwrap().len(), size);

    keys.deny(false);
    let store = Store::open_keyed(&path, &keys).unwrap();
    assert_eq!(messages(&store), 1);
    assert_eq!(DeviceKeys::load_or_create(&keys).unwrap().0, device_id);
}

#[test]
fn a_first_start_with_a_denied_keychain_creates_nothing() {
    let dir = TempDir::new("denied-fresh");
    let path = dir.file("chat.db");
    let keys = Guarded::default();
    keys.deny(true);
    assert_eq!(
        Store::open_keyed(&path, &keys).err(),
        Some(ChatError::KeychainDenied)
    );
    assert_eq!(
        DeviceKeys::load_or_create(&keys).err(),
        Some(ChatError::KeychainDenied)
    );
    assert!(!path.exists());
    assert_eq!(keys.keys.load(KeyName::Database).unwrap(), None);
    assert_eq!(keys.keys.load(KeyName::DeviceId).unwrap(), None);
}

#[test]
fn the_denied_codes_are_stable_on_the_wire() {
    assert_eq!(
        serde_json::to_value(ChatError::KeychainDenied).unwrap(),
        serde_json::json!({"code": "keychain_denied"})
    );
    assert_eq!(
        serde_json::to_value(ChatPhase::KeychainDenied).unwrap(),
        serde_json::json!("keychain_denied")
    );
    assert_eq!(
        ChatError::from_server(&serde_json::json!({"error": "chat_unimplemented"})),
        ChatError::Protocol
    );
}
