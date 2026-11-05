mod support;

use std::time::Duration;

use sc_chat::ChatPhase;
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
