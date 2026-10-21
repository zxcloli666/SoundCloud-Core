use std::sync::Arc;

use sc_chat::api::wire::LinkStart;
use sc_chat::api::{BindSigner, Signer};
use sc_chat::crypto::DeviceKeys;
use sc_chat::{ChatError, LinkClaimed, LinkCreated, LinkMode, LinkState, LoginState};
use uuid::Uuid;

use crate::support::{Client, Person, Stack};

fn fresh_device() -> BindSigner {
    BindSigner {
        device_id: Uuid::new_v4(),
        keys: Arc::new(DeviceKeys::generate().unwrap()),
        target_session: None,
    }
}

fn source_of(client: &Client) -> BindSigner {
    BindSigner {
        device_id: client.person.device_id,
        keys: client.person.keys.clone(),
        target_session: Some(client.person.session_id.to_string()),
    }
}

fn linked(owner: &Person, device: &BindSigner, session: &str) -> Person {
    Person {
        session_id: Uuid::parse_str(session).unwrap(),
        device_id: device.device_id,
        keys: device.keys.clone(),
        ..owner.clone()
    }
}

#[tokio::test]
#[ignore]
async fn an_unbound_session_reads_config_and_is_sent_to_relogin() {
    let stack = Stack::from_env();
    let owner = stack.person("old");
    let unbound = Person {
        session_id: stack.unbound_session(&owner),
        ..owner
    };
    let client = Client::on(
        vec![(sc_chat::Tier::Direct, stack.origin.clone())],
        &unbound,
    );
    assert!(client.api.config().await.unwrap().enabled);
    assert_eq!(
        client.register().await.err(),
        Some(ChatError::ReloginRequired)
    );
}

#[tokio::test]
#[ignore]
async fn a_session_signed_by_another_device_is_bound_other() {
    let stack = Stack::from_env();
    let owner = stack.person("twin");
    let other = fresh_device();
    let impostor = Person {
        device_id: other.device_id,
        keys: other.keys.clone(),
        ..owner
    };
    let client = Client::on(
        vec![(sc_chat::Tier::Direct, stack.origin.clone())],
        &impostor,
    );
    assert_eq!(
        client.register().await.err(),
        Some(ChatError::SessionBoundOther)
    );
}

#[tokio::test]
#[ignore]
async fn a_pull_link_claimed_by_the_signed_in_device_binds_the_new_one() {
    let stack = Stack::from_env();
    let source = Client::up(&stack, &stack.person("source")).await;
    let fresh = fresh_device();
    let created = source.api.link_create_pull(&fresh).await.unwrap();
    let claimed = source
        .api
        .link_claim_push(&source_of(&source), &created.claim_token)
        .await
        .unwrap();
    assert_eq!(claimed.mode, LinkMode::Pull);
    let polled = source
        .api
        .link_status(&fresh, &created.link_request_id)
        .await
        .unwrap();
    assert_eq!(polled.status, LinkState::Claimed);
    let session = polled
        .session_id
        .expect("the new device learns its session");

    let new_device = Client::up(&stack, &linked(&source.person, &fresh, &session)).await;
    assert_eq!(new_device.engine.devices().await.unwrap().len(), 2);
}

#[tokio::test]
#[ignore]
async fn a_push_link_from_the_signed_in_device_binds_the_new_one() {
    let stack = Stack::from_env();
    let source = Client::up(&stack, &stack.person("pusher")).await;
    let start = LinkStart {
        mode: LinkMode::Push,
    };
    let created: LinkCreated = source
        .api
        .json_as(
            &Signer::Bind(source_of(&source)),
            "POST",
            "/auth/link/create",
            "",
            Some(&start),
        )
        .await
        .unwrap();
    let fresh = fresh_device();
    let claimed: LinkClaimed = source
        .api
        .link_claim_push(&fresh, &created.claim_token)
        .await
        .unwrap();
    assert_eq!(claimed.mode, LinkMode::Push);

    let new_device = Client::up(&stack, &linked(&source.person, &fresh, &claimed.session_id)).await;
    assert_eq!(new_device.engine.devices().await.unwrap().len(), 2);
}

#[tokio::test]
#[ignore]
async fn a_sealed_relogin_of_the_current_session_stores_its_bind_and_shows_the_result_only_to_that_device()
 {
    let stack = Stack::from_env();
    stack.oauth_app();
    let owner = stack.person("back");
    let current = stack.unbound_session(&owner);
    let client = Client::on(
        vec![(sc_chat::Tier::Direct, stack.origin.clone())],
        &Person {
            session_id: current,
            ..owner.clone()
        },
    );
    let device = BindSigner {
        target_session: Some(current.to_string()),
        ..fresh_device()
    };
    let begun = client.api.login_begin(&device).await.unwrap();
    assert!(begun.url.starts_with("https://"));
    let stored = stack.sql(&format!(
        "SELECT target_session_id::text || ' ' || (chat_bind IS NOT NULL)::text FROM login_requests WHERE id = '{}'",
        begun.login_request_id
    ));
    assert_eq!(stored, format!("{current} true"));
    let polled = client
        .api
        .login_status(&device, &begun.login_request_id)
        .await
        .unwrap();
    assert_eq!(polled.status, LoginState::Pending);

    stack.sql(&format!(
        "UPDATE login_requests SET status = 'completed', result_session_id = target_session_id WHERE id = '{}'",
        begun.login_request_id
    ));
    let stranger = client
        .api
        .login_status(&fresh_device(), &begun.login_request_id)
        .await
        .unwrap();
    assert_eq!(stranger.session_id, None);
    let mine = client
        .api
        .login_status(&device, &begun.login_request_id)
        .await
        .unwrap();
    assert_eq!(mine.session_id, Some(current.to_string()));

    stack.sql(&format!(
        "UPDATE sessions SET chat_bind = (SELECT chat_bind FROM login_requests WHERE id = '{}') WHERE id = '{current}'",
        begun.login_request_id
    ));
    let back = Client::up(&stack, &linked(&owner, &device, &current.to_string())).await;
    let devices = back.engine.devices().await.unwrap();
    assert!(
        devices
            .iter()
            .any(|seen| seen.id == device.device_id && seen.current)
    );
}
