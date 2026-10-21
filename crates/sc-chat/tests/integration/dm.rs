use sc_chat::api::wire::NewConversation;
use sc_chat::{ConvId, ConversationKind, DeliveryState};

use crate::support::{Client, Stack};

pub fn direct(with: &str) -> NewConversation {
    NewConversation {
        kind: ConversationKind::Direct,
        members: vec![with.to_owned()],
        meta_ct: None,
        meta_epoch: None,
    }
}

fn state_of(client: &Client, conversation_id: ConvId, text: &str) -> Option<DeliveryState> {
    client
        .message(conversation_id, text)
        .map(|message| message.state)
}

#[tokio::test]
#[ignore]
async fn a_dm_carries_text_both_ways_with_delivered_and_read_marks() {
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
    ann.send(view.id, "hi bob").await.unwrap();
    assert_eq!(state_of(&ann, view.id, "hi bob"), Some(DeliveryState::Sent));

    bob.sync_until("bob gets the chat", |bob| bob.texts(view.id) == ["hi bob"])
        .await;
    bob.sync().await.unwrap();
    ann.sync_until("ann sees her message delivered", |ann| {
        state_of(ann, view.id, "hi bob") == Some(DeliveryState::Delivered)
    })
    .await;

    bob.send(view.id, "hi ann").await.unwrap();
    ann.sync_until("ann gets the answer", |ann| {
        ann.texts(view.id) == ["hi bob", "hi ann"]
    })
    .await;
    ann.sync().await.unwrap();
    let seen = bob.messages(view.id);
    assert!(!seen[0].mine && seen[1].mine);
    bob.sync_until("bob sees his answer delivered", |bob| {
        state_of(bob, view.id, "hi ann") == Some(DeliveryState::Delivered)
    })
    .await;

    bob.engine.queue_read(view.id, seen[1].seq.unwrap());
    bob.sync().await.unwrap();
    ann.sync_until("ann sees bob read her message", |ann| {
        state_of(ann, view.id, "hi bob") == Some(DeliveryState::Read)
    })
    .await;
    let last = ann.messages(view.id)[1].seq.unwrap();
    ann.engine.queue_read(view.id, last);
    ann.sync().await.unwrap();
    bob.sync_until("bob sees ann read his answer", |bob| {
        state_of(bob, view.id, "hi ann") == Some(DeliveryState::Read)
    })
    .await;
    assert_eq!(bob.conversation(view.id).unwrap().unread, 0);
    assert_eq!(ann.conversation(view.id).unwrap().unread, 0);
}
