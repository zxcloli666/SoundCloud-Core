mod support;

use sc_chat::{ChatError, ChatPhase, KeyName, LinkMode, LoginTarget, RichText, SendInput, Tier};
use support::{Rig, settle, until};

#[tokio::test]
async fn a_client_without_a_session_is_signed_out() {
    let rig = Rig::new();
    until("signed out", || {
        rig.client.status().phase == ChatPhase::SignedOut
    })
    .await;
    assert!(rig.events.statuses() > 0);
    assert_eq!(rig.transport.requests(), 0);
}

#[tokio::test]
async fn a_sealed_login_walks_every_tier_and_reports_offline_on_a_bad_answer() {
    let rig = Rig::new();
    let begun = settle(rig.client.login_begin(LoginTarget::Fresh)).await;
    assert_eq!(begun.err(), Some(ChatError::Offline));
    assert_eq!(rig.transport.tiers_seen(), vec![Tier::Direct, Tier::Relay]);
    let mut names = rig.keys.names();
    names.sort_by_key(|name| name.as_str());
    assert_eq!(
        names,
        vec![KeyName::DeviceAuth, KeyName::DeviceBox, KeyName::DeviceId]
    );
}

#[tokio::test]
async fn steps_that_speak_for_the_session_need_one() {
    let rig = Rig::new();
    let current = settle(rig.client.login_begin(LoginTarget::Current)).await;
    assert_eq!(current.err(), Some(ChatError::NotReady));
    let push = settle(rig.client.link_create(LinkMode::Push)).await;
    assert_eq!(push.err(), Some(ChatError::NotReady));
    let pull = settle(rig.client.link_claim(LinkMode::Pull, "token".into())).await;
    assert_eq!(pull.err(), Some(ChatError::NotReady));
    assert_eq!(rig.transport.requests(), 0);
}

#[tokio::test]
async fn chat_calls_wait_for_the_store() {
    let rig = Rig::new();
    let input = SendInput {
        conversation_id: uuid::Uuid::new_v4(),
        client_id: None,
        text: RichText {
            text: "hi".into(),
            spans: Vec::new(),
        },
        reply_to: None,
        embeds: Vec::new(),
        forward: None,
    };
    assert_eq!(
        settle(rig.client.send(input)).await.err(),
        Some(ChatError::NotReady)
    );
    assert_eq!(
        settle(rig.client.devices()).await.err(),
        Some(ChatError::NotReady)
    );
}

#[tokio::test]
async fn a_session_opens_the_keyed_store_and_signing_out_drops_the_device() {
    let rig = Rig::new();
    rig.client.set_session(Some("session".into()));
    until("the store opens", || rig.dir.join("chat.db").exists()).await;
    until("the config call goes out", || rig.transport.requests() > 0).await;
    assert_eq!(rig.client.status().phase, ChatPhase::Starting);
    assert_eq!(rig.keys.names().len(), 4);

    rig.client.set_session(None);
    until("the device keys are gone", || rig.keys.names().is_empty()).await;
    until("signed out", || {
        rig.client.status().phase == ChatPhase::SignedOut
    })
    .await;
}
