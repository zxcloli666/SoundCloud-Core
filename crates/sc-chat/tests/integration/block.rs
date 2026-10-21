use sc_chat::{ChatError, DeliveryState};

use crate::dm::direct;
use crate::support::{Client, Stack};

#[tokio::test]
#[ignore]
async fn a_block_stops_new_messages_and_an_unblock_lets_them_through() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    stack.follow(&bob.person, &ann.person);
    let view = ann
        .api
        .create_conversation(&direct(&bob.person.user_id))
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    assert_eq!(ann.send(view.id, "before").await, Ok(1));
    bob.sync_until("bob gets the first one", |bob| {
        bob.texts(view.id) == ["before"]
    })
    .await;

    bob.api.block(&ann.person.user_id).await.unwrap();
    assert_eq!(ann.raw_post(view.id).await, Err(ChatError::Unavailable));
    assert_eq!(ann.send(view.id, "while blocked").await, Ok(0));
    let held = ann.outbox.pending(view.id).unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].state, DeliveryState::Failed);
    let settings = bob.api.settings().await.unwrap();
    assert!(
        settings
            .blocked
            .iter()
            .any(|blocked| blocked.user_id == ann.person.user_id)
    );

    bob.api.unblock(&ann.person.user_id).await.unwrap();
    assert_eq!(ann.send(view.id, "after").await, Ok(1));
    bob.sync_until("bob gets only the messages outside the block", |bob| {
        bob.texts(view.id) == ["before", "after"]
    })
    .await;
}
