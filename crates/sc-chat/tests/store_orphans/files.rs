use sc_chat::store::{Store, remove_orphans};

use super::store_support::{MemoryKeys, TempDir};
use super::{lose_key_after, messages, names, orphans, parked};

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

    let left = parked(&dir, "");
    assert!(left.is_empty(), "{left:?}");
}

#[cfg(unix)]
#[test]
fn a_wipe_goes_on_past_an_orphan_it_cannot_remove() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    use sc_chat::ChatError;

    use super::store_support::{PEER, conv, post};

    let dir = TempDir::new("wipe-stuck");
    let path = dir.file("chat.db");
    let store = Store::open_keyed(&path, &MemoryKeys::default()).unwrap();
    store
        .put_messages(&[post(conv(), 1, PEER, "стёрто")])
        .unwrap();
    std::fs::write(dir.file("chat.db.orphan-4"), b"old").unwrap();
    std::fs::set_permissions(&dir.0, Permissions::from_mode(0o555)).unwrap();

    let wiped = store.wipe();
    let removed = remove_orphans(&path);
    std::fs::set_permissions(&dir.0, Permissions::from_mode(0o755)).unwrap();

    assert_eq!(wiped, Ok(()));
    assert_eq!(messages(&store), 0);
    assert_eq!(removed, Err(ChatError::Store));
}

#[test]
fn only_exact_orphan_files_are_removed() {
    let dir = TempDir::new("exact");
    let path = dir.file("chat.db");
    for name in [
        "chat.db.orphan-4",
        "chat.db.orphan-4-wal",
        "chat.db.orphan-5-shm",
        "chat.db.orphan-12x",
        "chat.db.orphan-",
        "chat.db.orphan--wal",
        "chat.db.orphan-3-walx",
    ] {
        std::fs::write(dir.file(name), b"kept").unwrap();
    }
    std::fs::create_dir_all(dir.file("chat.db.orphan-6").join("inside")).unwrap();

    assert_eq!(remove_orphans(&path), Ok(()));

    assert_eq!(
        names(&dir),
        [
            "chat.db.orphan-",
            "chat.db.orphan--wal",
            "chat.db.orphan-12x",
            "chat.db.orphan-3-walx",
            "chat.db.orphan-6",
        ]
    );
    assert!(dir.file("chat.db.orphan-6").join("inside").exists());
}

#[test]
fn a_stray_wal_is_parked_apart_from_every_orphan() {
    let dir = TempDir::new("strays");
    let path = dir.file("chat.db");
    std::fs::write(dir.file("chat.db.orphan-7"), b"db").unwrap();
    std::fs::write(dir.file("chat.db-wal"), b"stray wal").unwrap();

    let store = Store::open_keyed(&path, &MemoryKeys::default()).unwrap();

    assert_eq!(messages(&store), 0);
    assert!(!dir.file("chat.db.orphan-7-wal").exists());
    let wal = parked(&dir, "-wal");
    assert_eq!(wal.len(), 1);
    assert!(!dir.file(wal[0].strip_suffix("-wal").unwrap()).exists());
    assert_eq!(std::fs::read(dir.file(&wal[0])).unwrap(), b"stray wal");
}

#[test]
fn parked_strays_are_pruned_apart_from_orphans() {
    let dir = TempDir::new("stray-prune");
    let path = dir.file("chat.db");
    for name in [
        "chat.db.orphan-1",
        "chat.db.orphan-2-wal",
        "chat.db.orphan-3-wal",
        "chat.db.orphan-4-shm",
        "chat.db-wal",
    ] {
        std::fs::write(dir.file(name), b"old").unwrap();
    }

    drop(Store::open_keyed(&path, &MemoryKeys::default()).unwrap());

    assert!(dir.file("chat.db.orphan-1").exists());
    assert!(!dir.file("chat.db.orphan-2-wal").exists());
    assert!(!dir.file("chat.db.orphan-3-wal").exists());
    assert!(dir.file("chat.db.orphan-4-shm").exists());
    assert_eq!(parked(&dir, "-wal").len(), 1);
}

#[cfg(unix)]
#[test]
fn a_database_path_that_cannot_be_checked_keeps_its_wal() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    use sc_chat::ChatError;

    let dir = TempDir::new("unchecked");
    let path = dir.file("chat.db");
    let locked = dir.file("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("chat.db"), b"db").unwrap();
    std::os::unix::fs::symlink(locked.join("chat.db"), &path).unwrap();
    std::fs::write(dir.file("chat.db-wal"), b"live wal").unwrap();
    std::fs::set_permissions(&locked, Permissions::from_mode(0o000)).unwrap();

    let opened = Store::open_keyed(&path, &MemoryKeys::default()).err();
    std::fs::set_permissions(&locked, Permissions::from_mode(0o755)).unwrap();

    assert_eq!(opened, Some(ChatError::Store));
    assert_eq!(std::fs::read(dir.file("chat.db-wal")).unwrap(), b"live wal");
}
