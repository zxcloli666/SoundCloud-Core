use sc_chat::store::{ORPHANS_KEPT, Store};
use sc_chat::{KeyName, KeyStore};

use super::store_support::{MemoryKeys, PEER, TempDir, conv, post};
use super::{lose_key_after, master, messages, orphans};

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
fn a_failed_move_puts_the_database_back() {
    use sc_chat::ChatError;

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

#[cfg(target_os = "macos")]
#[test]
fn an_orphan_that_cannot_be_pruned_does_not_stop_the_start() {
    let dir = TempDir::new("prune");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let stuck = dir.file("chat.db.orphan-1");
    std::fs::write(&stuck, b"db").unwrap();
    std::fs::write(dir.file("chat.db.orphan-2"), b"db").unwrap();
    let lost = lose_key_after(&path, &keys, "живой");
    pin(&stuck, true);

    let opened = Store::open_keyed(&path, &keys);
    pin(&stuck, false);

    assert_eq!(messages(&opened.unwrap()), 0);
    let newest = orphans(&dir).pop().unwrap();
    assert_eq!(messages(&Store::open(&newest, &lost).unwrap()), 1);
    assert!(stuck.exists());
}

#[cfg(target_os = "macos")]
fn pin(file: &std::path::Path, pinned: bool) {
    let flag = if pinned { "uchg" } else { "nouchg" };
    let done = std::process::Command::new("chflags")
        .arg(flag)
        .arg(file)
        .status()
        .unwrap();
    assert!(done.success());
}
