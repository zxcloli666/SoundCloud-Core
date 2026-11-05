mod support;

use std::time::{Duration, Instant};

use sc_chat::ChatPhase;
use support::{Rig, until};

async fn unavailable() -> Rig {
    let rig = Rig::new();
    rig.keys.break_down(true);
    rig.client.set_session(Some("session".into()));
    until("the keychain unavailable phase", || {
        rig.client.status().phase == ChatPhase::KeychainUnavailable
    })
    .await;
    rig
}

#[tokio::test]
async fn an_unavailable_keychain_shows_its_phase_and_retries_on_its_own() {
    let rig = unavailable().await;
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
async fn the_phase_stays_unavailable_across_attempts() {
    let rig = unavailable().await;
    let before = rig.events.phases().len();
    let asked = rig.keys.calls();
    until("a retry without the user", || rig.keys.calls() > asked).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(rig.events.phases()[before..], []);
    assert_eq!(rig.client.status().phase, ChatPhase::KeychainUnavailable);
}

#[tokio::test]
async fn an_app_that_retries_in_a_loop_reaches_the_keychain_once_per_floor() {
    let rig = unavailable().await;
    let asked = rig.keys.calls();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2_500) {
        rig.client.retry_keychain();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let attempts = rig.keys.calls() - asked;
    assert!((1..=2).contains(&attempts), "{attempts}");
}

#[tokio::test]
async fn retry_keychain_cuts_a_long_wait_down_to_the_floor() {
    let rig = unavailable().await;
    let asked = rig.keys.calls();
    until("the second attempt", || rig.keys.calls() > asked).await;
    rig.keys.break_down(false);
    let retried = Instant::now();
    rig.client.retry_keychain();

    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    assert!(retried.elapsed() < Duration::from_millis(3_000));
}
