mod store_support;

use std::path::{Path, PathBuf};

use sc_chat::store::{ORPHANS_KEPT, Store};
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

fn messages(store: &Store) -> i64 {
    store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap()
}

fn orphans(dir: &TempDir) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_string_lossy();
            name.starts_with("chat.db.orphan-")
                && !name.ends_with("-wal")
                && !name.ends_with("-shm")
        })
        .collect();
    found.sort();
    found
}

fn master(keys: &MemoryKeys) -> [u8; 32] {
    let saved = keys.load(KeyName::Database).unwrap().unwrap();
    saved.as_slice().try_into().unwrap()
}

fn lose_key_after(path: &Path, keys: &MemoryKeys, text: &str) -> [u8; 32] {
    let store = Store::open_keyed(path, keys).unwrap();
    store.put_messages(&[post(conv(), 1, PEER, text)]).unwrap();
    drop(store);
    let lost = master(keys);
    keys.delete(KeyName::Database).unwrap();
    lost
}

#[test]
fn a_lost_key_sets_the_old_database_aside_and_starts_fresh() {
    let dir = TempDir::new("lost");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let lost = lose_key_after(&path, &keys, "останется в стороне");

    let store = Store::open_keyed(&path, &keys).unwrap();

    assert_eq!(messages(&store), 0);
    let kept = orphans(&dir);
    assert_eq!(kept.len(), 1);
    assert_eq!(messages(&Store::open(&kept[0], &lost).unwrap()), 1);
}

#[test]
fn a_replaced_key_sets_the_old_database_aside_and_starts_fresh() {
    let dir = TempDir::new("replaced");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let store = Store::open_keyed(&path, &keys).unwrap();
    store
        .put_messages(&[post(conv(), 1, PEER, "старый ключ")])
        .unwrap();
    drop(store);
    let old = master(&keys);
    keys.save(KeyName::Database, &[8; 32]).unwrap();

    let store = Store::open_keyed(&path, &keys).unwrap();

    assert_eq!(messages(&store), 0);
    let kept = orphans(&dir);
    assert_eq!(kept.len(), 1);
    assert_eq!(messages(&Store::open(&kept[0], &old).unwrap()), 1);
}

#[test]
fn only_the_newest_orphans_are_kept() {
    let dir = TempDir::new("orphans");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let mut lost = Vec::new();
    for text in ["первый", "второй", "третий"] {
        lost.push(lose_key_after(&path, &keys, text));
        drop(Store::open_keyed(&path, &keys).unwrap());
    }

    let kept = orphans(&dir);
    assert_eq!(kept.len(), ORPHANS_KEPT);
    for (orphan, key) in kept.iter().zip(&lost[1..]) {
        assert_eq!(messages(&Store::open(orphan, key).unwrap()), 1);
    }
    assert!(Store::open(&kept[0], &lost[0]).is_err());
}

#[test]
fn a_wiped_database_without_its_key_is_not_kept() {
    let dir = TempDir::new("wiped");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let store = Store::open_keyed(&path, &keys).unwrap();
    store
        .put_messages(&[post(conv(), 1, PEER, "стёрто")])
        .unwrap();
    store.wipe().unwrap();
    drop(store);
    keys.delete(KeyName::Database).unwrap();

    let store = Store::open_keyed(&path, &keys).unwrap();

    assert_eq!(messages(&store), 0);
    assert!(orphans(&dir).is_empty());
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
