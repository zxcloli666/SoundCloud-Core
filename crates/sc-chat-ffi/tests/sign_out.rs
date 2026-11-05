mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sc_chat::store::Store;
use sc_chat::{ChatPhase, ConversationKind, KeyName};
use sc_chat_ffi::ChatKeys;
use support::{Rig, summary, until};

#[tokio::test]
async fn a_sign_out_while_the_store_opens_sends_nothing_for_the_old_session() {
    let rig = Rig::new();
    let client = Arc::downgrade(&rig.client);
    let fired = Arc::new(AtomicBool::new(false));
    let once = fired.clone();
    rig.keys.on_ask(move || {
        if !once.swap(true, Ordering::SeqCst)
            && let Some(client) = client.upgrade()
        {
            client.set_session(None);
        }
    });

    rig.client.set_session(Some("session".into()));
    until("the sign-out lands", || fired.load(Ordering::SeqCst)).await;
    until("signed out", || {
        rig.client.status().phase == ChatPhase::SignedOut
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(rig.transport.requests(), 0);
    assert_eq!(rig.client.status().device_id, None);
    assert_eq!(rig.client.status().phase, ChatPhase::SignedOut);
}

#[tokio::test]
async fn signing_out_removes_the_orphans_next_to_the_store() {
    let rig = Rig::new();
    std::fs::create_dir_all(&rig.dir).unwrap();
    for name in [
        "chat.db.orphan-5",
        "chat.db.orphan-5-wal",
        "chat.db.orphan-6-shm",
    ] {
        std::fs::write(rig.dir.join(name), b"old metadata").unwrap();
    }
    rig.client.set_session(Some("session".into()));
    until("the store opens", || rig.dir.join("chat.db").exists()).await;

    rig.client.set_session(None);
    until("the device keys are gone", || rig.keys.names().is_empty()).await;

    until("the orphans are gone", || {
        std::fs::read_dir(&rig.dir).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".orphan-")
        })
    })
    .await;
}

#[tokio::test]
async fn a_sign_out_with_no_store_open_deletes_the_database() {
    let rig = Rig::new();
    std::fs::create_dir_all(&rig.dir).unwrap();
    let master = [7; 32];
    let store = Store::open(&rig.dir.join("chat.db"), &master).unwrap();
    store
        .put_conversations(&[summary(uuid::Uuid::new_v4(), ConversationKind::Group)])
        .unwrap();
    drop(store);
    rig.keys.save(KeyName::Database, master.to_vec()).unwrap();
    rig.keys.deny(true);
    rig.client.set_session(Some("session".into()));
    until("the keychain is denied", || {
        rig.client.status().phase == ChatPhase::KeychainDenied
    })
    .await;

    rig.client.set_session(None);

    until("the database is gone", || !rig.dir.join("chat.db").exists()).await;
    assert!(!rig.dir.join("chat.db-wal").exists());
    assert!(!rig.dir.join("chat.db-shm").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn a_sign_out_erases_the_keys_even_when_an_orphan_cannot_go() {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    struct Locked(std::path::PathBuf);

    impl Drop for Locked {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, Permissions::from_mode(0o755));
        }
    }

    let rig = Rig::new();
    rig.with_store().await;
    std::fs::write(rig.dir.join("chat.db.orphan-5"), b"old metadata").unwrap();
    std::fs::set_permissions(&rig.dir, Permissions::from_mode(0o555)).unwrap();
    let _locked = Locked(rig.dir.clone());

    rig.client.set_session(None);

    until("the device keys are gone", || rig.keys.names().is_empty()).await;
    assert!(rig.dir.join("chat.db.orphan-5").exists());
}
