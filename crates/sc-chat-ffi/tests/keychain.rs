mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sc_chat::{ChatPhase, CoreEvent};
use support::{Rig, until};

#[tokio::test]
async fn a_denied_keychain_parks_the_chat_until_the_user_retries() {
    let rig = Rig::new();
    rig.keys.deny(true);
    rig.client.set_session(Some("session".into()));
    until("the keychain denied phase", || {
        rig.client.status().phase == ChatPhase::KeychainDenied
    })
    .await;
    let asked = rig.keys.calls();
    assert_eq!(asked, 1);

    rig.client.network_changed();
    rig.client.set_visible(true);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.keys.calls(), asked);
    assert_eq!(rig.client.status().phase, ChatPhase::KeychainDenied);
    assert_eq!(rig.transport.requests(), 0);

    rig.client.retry_keychain();
    until("one more keychain ask", || rig.keys.calls() > asked).await;
    until("parked again", || {
        rig.client.status().phase == ChatPhase::KeychainDenied
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.keys.calls(), asked + 1);

    rig.keys.deny(false);
    rig.client.retry_keychain();
    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    until("the config call goes out", || rig.transport.requests() > 0).await;
    assert_ne!(rig.client.status().phase, ChatPhase::KeychainDenied);
}

#[tokio::test]
async fn retry_keychain_does_nothing_outside_the_denied_phase() {
    let rig = Rig::new();
    until("signed out", || {
        rig.client.status().phase == ChatPhase::SignedOut
    })
    .await;
    rig.client.retry_keychain();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.client.status().phase, ChatPhase::SignedOut);
    assert_eq!(rig.keys.calls(), 0);
}

#[tokio::test]
async fn a_retry_from_the_denied_status_event_restarts() {
    let rig = Rig::new();
    rig.keys.deny(true);
    let client = Arc::downgrade(&rig.client);
    let keys = rig.keys.clone();
    let retried = Arc::new(AtomicBool::new(false));
    let once = retried.clone();
    rig.events.on(move |event| {
        let CoreEvent::Status(status) = event else {
            return;
        };
        if status.phase == ChatPhase::KeychainDenied && !once.swap(true, Ordering::SeqCst) {
            keys.deny(false);
            if let Some(client) = client.upgrade() {
                client.retry_keychain();
            }
        }
    });

    rig.client.set_session(Some("session".into()));
    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    assert!(retried.load(Ordering::SeqCst));
    assert_ne!(rig.client.status().phase, ChatPhase::KeychainDenied);
}

#[tokio::test]
async fn an_unavailable_keychain_shows_its_phase_and_retries_on_its_own() {
    let rig = Rig::new();
    rig.keys.break_down(true);
    rig.client.set_session(Some("session".into()));
    until("the keychain unavailable phase", || {
        rig.client.status().phase == ChatPhase::KeychainUnavailable
    })
    .await;
    assert!(!rig.client.status().online);
    let asked = rig.keys.calls();
    until("a retry without the user", || rig.keys.calls() > asked).await;
    assert_eq!(rig.client.status().phase, ChatPhase::KeychainUnavailable);

    rig.keys.break_down(false);
    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    until("the config call goes out", || rig.transport.requests() > 0).await;
    assert_ne!(rig.client.status().phase, ChatPhase::KeychainUnavailable);
}

#[tokio::test]
async fn retry_keychain_skips_the_wait_of_an_unavailable_keychain() {
    let rig = Rig::new();
    rig.keys.break_down(true);
    rig.client.set_session(Some("session".into()));
    until("the keychain unavailable phase", || {
        rig.client.status().phase == ChatPhase::KeychainUnavailable
    })
    .await;
    rig.keys.break_down(false);
    let retried = Instant::now();
    rig.client.retry_keychain();
    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    assert!(retried.elapsed() < Duration::from_millis(1_500));
}
