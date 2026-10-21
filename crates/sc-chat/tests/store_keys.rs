mod store_support;

use sc_chat::store::Store;
use sc_chat::{ChatError, KeyName, KeyStore};
use store_support::{MemoryKeys, PEER, TempDir, conv, post};

#[test]
fn the_database_key_comes_from_the_keystore_and_stays() {
    let dir = TempDir::new("keys");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let id = conv();
    Store::open_keyed(&path, &keys)
        .unwrap()
        .put_messages(&[post(id, 1, PEER, "сохранено")])
        .unwrap();
    let saved = keys.load(KeyName::Database).unwrap().unwrap();
    assert_eq!(saved.len(), 32);
    let store = Store::open_keyed(&path, &keys).unwrap();
    assert_eq!(keys.load(KeyName::Database).unwrap().unwrap(), saved);
    let count: i64 = store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn a_wrong_key_is_refused() {
    let dir = TempDir::new("wrong");
    let path = dir.file("chat.db");
    drop(Store::open(&path, &[1; 32]).unwrap());
    assert_eq!(Store::open(&path, &[2; 32]).err(), Some(ChatError::Crypto));
    assert!(Store::open(&path, &[1; 32]).is_ok());
}

#[test]
fn a_lost_key_starts_a_fresh_database() {
    let dir = TempDir::new("lost");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    Store::open_keyed(&path, &keys)
        .unwrap()
        .put_messages(&[post(conv(), 1, PEER, "потеряется")])
        .unwrap();
    keys.delete(KeyName::Database).unwrap();
    let store = Store::open_keyed(&path, &keys).unwrap();
    let count: i64 = store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn a_replaced_key_starts_a_fresh_database() {
    let dir = TempDir::new("replaced");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    drop(Store::open_keyed(&path, &keys).unwrap());
    keys.save(KeyName::Database, &[8; 32]).unwrap();
    assert!(Store::open_keyed(&path, &keys).is_ok());
}

#[cfg(unix)]
#[test]
fn the_database_files_are_for_the_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new("mode");
    let path = dir.file("chat.db");
    std::fs::write(&path, b"").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let keys = MemoryKeys::default();
    let store = Store::open_keyed(&path, &keys).unwrap();
    store
        .put_messages(&[post(conv(), 1, PEER, "только мне")])
        .unwrap();
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    for file in [path.clone(), wal.into()] {
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{}", file.display());
    }
}
