mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sc_chat::ChatPhase;
use support::{Rig, until};

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
