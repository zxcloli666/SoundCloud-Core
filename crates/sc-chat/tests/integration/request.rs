use sc_chat::api::wire::SettingsChange;
use sc_chat::{ChatError, DmPolicy};

use crate::dm::direct;
use crate::support::{Client, Stack};

#[tokio::test]
#[ignore]
async fn a_stranger_from_an_hour_old_session_lands_in_requests_with_three_posts_and_a_decline_closes_it()
 {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let dee = Client::up(&stack, &stack.person("dee")).await;
    let open = SettingsChange {
        dm_policy: Some(DmPolicy::Everyone),
        ..SettingsChange::default()
    };
    dee.api.put_settings(&open).await.unwrap();
    assert_eq!(
        ann.api
            .create_conversation(&direct(&dee.person.user_id))
            .await
            .err(),
        Some(ChatError::TooYoung)
    );
    stack.age_session(&ann.person, 2);
    let view = ann
        .api
        .create_conversation(&direct(&dee.person.user_id))
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    for text in ["hello", "are you there", "last one"] {
        assert_eq!(ann.send(view.id, text).await, Ok(1));
    }
    assert_eq!(ann.raw_post(view.id).await, Err(ChatError::RequestLimit));

    dee.sync_until("dee sees one request", |dee| {
        dee.engine.status().requests == 1
            && dee
                .conversation(view.id)
                .is_some_and(|summary| !summary.accepted)
    })
    .await;
    let listed = dee.api.requests().await.unwrap();
    assert!(
        listed
            .requests
            .iter()
            .any(|request| request.conversation_id == view.id
                && request.from == ann.person.user_id)
    );
    dee.api.decline(view.id).await.unwrap();
    assert_eq!(ann.raw_post(view.id).await, Err(ChatError::Unavailable));
    dee.sync_until("the request count drops", |dee| {
        dee.engine.status().requests == 0
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn an_accepted_request_moves_to_the_list_and_lifts_the_three_post_cap() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let dee = Client::up(&stack, &stack.person("dee")).await;
    let open = SettingsChange {
        dm_policy: Some(DmPolicy::Everyone),
        ..SettingsChange::default()
    };
    dee.api.put_settings(&open).await.unwrap();
    stack.age_session(&ann.person, 2);
    let view = ann
        .api
        .create_conversation(&direct(&dee.person.user_id))
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    for text in ["one", "two", "three"] {
        assert_eq!(ann.send(view.id, text).await, Ok(1));
    }
    assert_eq!(ann.raw_post(view.id).await, Err(ChatError::RequestLimit));
    dee.sync_until("dee sees the request", |dee| {
        dee.engine.status().requests == 1 && dee.texts(view.id) == ["one", "two", "three"]
    })
    .await;

    dee.api.accept(view.id).await.unwrap();
    dee.sync_until("the request becomes a chat", |dee| {
        dee.engine.status().requests == 0
            && dee
                .conversation(view.id)
                .is_some_and(|summary| summary.accepted)
    })
    .await;
    assert!(dee.api.requests().await.unwrap().requests.is_empty());
    assert_eq!(ann.send(view.id, "four").await, Ok(1));
    assert_eq!(dee.send(view.id, "hi ann").await, Ok(1));
    ann.sync_until("ann reads the answer", |ann| {
        ann.texts(view.id).last().map(String::as_str) == Some("hi ann")
    })
    .await;
    dee.sync_until("dee gets the post past the cap", |dee| {
        dee.texts(view.id).iter().any(|text| text == "four")
    })
    .await;
}
