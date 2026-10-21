use sc_chat::api::wire::{MemberChange, NewConversation};
use sc_chat::{ChatError, ConversationKind, MemberRole, MessageBody, SystemKind};

use crate::support::{Client, Stack};

#[tokio::test]
#[ignore]
async fn a_group_reaches_every_member_promotes_kicks_and_lets_a_member_leave() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    let cid = Client::up(&stack, &stack.person("cid")).await;
    stack.follow(&bob.person, &ann.person);
    stack.follow(&cid.person, &ann.person);
    let view = ann
        .api
        .create_conversation(&NewConversation {
            kind: ConversationKind::Group,
            members: vec![bob.person.user_id.clone(), cid.person.user_id.clone()],
            meta_ct: None,
            meta_epoch: None,
        })
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    assert_eq!(ann.send(view.id, "welcome").await, Ok(1));

    for member in [&bob, &cid] {
        member
            .sync_until("a member gets the first post", |client| {
                client.texts(view.id) == ["welcome"]
            })
            .await;
    }
    assert_eq!(cid.send(view.id, "thanks").await, Ok(1));
    for member in [&ann, &bob] {
        member
            .sync_until("the answer reaches the rest", |client| {
                client.texts(view.id) == ["welcome", "thanks"]
            })
            .await;
    }

    let promoted = MemberChange {
        role: Some(MemberRole::Admin),
        unban: false,
    };
    ann.api
        .update_member(view.id, &bob.person.user_id, &promoted)
        .await
        .unwrap();
    let bob_id = bob.person.user_id.clone();
    cid.sync_until("cid sees bob promoted", |cid| {
        cid.messages(view.id)
            .iter()
            .any(|message| match &message.body {
                MessageBody::System(event) => {
                    event.kind == SystemKind::Role
                        && event.role == Some(MemberRole::Admin)
                        && event.target.as_deref() == Some(bob_id.as_str())
                }
                _ => false,
            })
    })
    .await;

    let cid_id = cid.person.user_id.clone();
    bob.api
        .remove_member(view.id, &cid_id, false)
        .await
        .expect("an admin kicks a member");
    cid.sync_until("the kicked member loses the chat on the device", |cid| {
        cid.conversation(view.id).is_none() && cid.messages(view.id).is_empty()
    })
    .await;
    assert!(cid.events.gone().contains(&view.id));
    assert_eq!(
        cid.raw_post(view.id).await.err(),
        Some(ChatError::NotMember)
    );
    for member in [&ann, &bob] {
        member
            .sync_until("the rest see the kick", |client| {
                client.has_system(view.id, SystemKind::Kicked, &cid_id)
            })
            .await;
    }

    bob.api
        .remove_member(view.id, &bob_id, false)
        .await
        .expect("a member leaves");
    bob.sync_until("the member who left loses the chat", |bob| {
        bob.conversation(view.id).is_none()
    })
    .await;
    ann.sync_until("the owner sees the leave", |ann| {
        ann.systems(view.id).iter().any(|event| {
            event.kind == SystemKind::Left && event.actor.as_deref() == Some(bob_id.as_str())
        })
    })
    .await;
    assert_eq!(
        bob.raw_post(view.id).await.err(),
        Some(ChatError::NotMember)
    );
    assert_eq!(ann.send(view.id, "just us now").await, Ok(1));
    assert_eq!(ann.api.conversation(view.id).await.unwrap().member_count, 1);
}
