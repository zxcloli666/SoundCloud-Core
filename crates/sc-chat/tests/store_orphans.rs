mod store_support;

use std::path::{Path, PathBuf};

use sc_chat::store::{ORPHANS_KEPT, Store};
use sc_chat::{ChatError, KeyName, KeyStore};
use store_support::{MemoryKeys, PEER, TempDir, conv, post};

fn messages(store: &Store) -> i64 {
    store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap()
}

fn orphans(dir: &TempDir) -> Vec<PathBuf> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name()?.to_str()?.to_owned();
            let at = name.strip_prefix("chat.db.orphan-")?.parse().ok()?;
            Some((at, path))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
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

#[test]
fn a_wipe_removes_every_orphan_next_to_the_store() {
    let dir = TempDir::new("wipe-orphans");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    lose_key_after(&path, &keys, "чужие метаданные");
    let store = Store::open_keyed(&path, &keys).unwrap();
    std::fs::write(dir.file("chat.db.orphan-3-wal"), b"stray").unwrap();
    assert_eq!(orphans(&dir).len(), 1);

    store.wipe().unwrap();

    let left: Vec<String> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".orphan-"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[cfg(unix)]
#[test]
fn a_failed_move_puts_the_database_back() {
    let dir = TempDir::new("rollback");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let lost = lose_key_after(&path, &keys, "вернётся");
    std::fs::write(dir.file("chat.db-wal"), b"wal bytes").unwrap();
    std::fs::write(dir.file("chat.db.orphan-99999999999"), b"").unwrap();
    std::fs::create_dir_all(dir.file("chat.db.orphan-100000000000-wal").join("taken")).unwrap();

    assert_eq!(
        Store::open_keyed(&path, &keys).err(),
        Some(ChatError::Store)
    );

    assert!(!dir.file("chat.db.orphan-100000000000").exists());
    assert_eq!(
        std::fs::read(dir.file("chat.db-wal")).unwrap(),
        b"wal bytes"
    );
    std::fs::remove_file(dir.file("chat.db-wal")).unwrap();
    assert_eq!(messages(&Store::open(&path, &lost).unwrap()), 1);
}

#[test]
fn a_stray_wal_without_its_database_moves_next_to_the_newest_orphan() {
    let dir = TempDir::new("strays");
    let path = dir.file("chat.db");
    std::fs::write(dir.file("chat.db.orphan-7"), b"db").unwrap();
    std::fs::write(dir.file("chat.db-wal"), b"stray wal").unwrap();
    let keys = MemoryKeys::default();

    let store = Store::open_keyed(&path, &keys).unwrap();

    assert_eq!(messages(&store), 0);
    assert_eq!(
        std::fs::read(dir.file("chat.db.orphan-7-wal")).unwrap(),
        b"stray wal"
    );
}

#[test]
fn an_orphan_that_cannot_be_pruned_does_not_stop_the_start() {
    let dir = TempDir::new("prune");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    std::fs::create_dir_all(dir.file("chat.db.orphan-1").join("stuck")).unwrap();
    std::fs::write(dir.file("chat.db.orphan-2"), b"db").unwrap();
    let lost = lose_key_after(&path, &keys, "живой");

    let store = Store::open_keyed(&path, &keys).unwrap();

    assert_eq!(messages(&store), 0);
    let newest = orphans(&dir).pop().unwrap();
    assert_eq!(messages(&Store::open(&newest, &lost).unwrap()), 1);
    assert!(dir.file("chat.db.orphan-1").exists());
}
