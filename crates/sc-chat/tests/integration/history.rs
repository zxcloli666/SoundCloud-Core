use sc_chat::{ChatError, ConvId, HistoryQuery, MessageBody};

use crate::dm::direct;
use crate::support::{Client, Stack};

const EVERYTHING: HistoryQuery = HistoryQuery {
    before: None,
    after: None,
    limit: 50,
};

async fn reads_from_history(client: &Client, conversation_id: ConvId, text: &str) -> bool {
    client
        .engine
        .fetch_history(conversation_id, &EVERYTHING)
        .await
        .unwrap()
        .messages
        .iter()
        .any(|message| matches!(&message.body, MessageBody::Text(rich) if rich.text == text))
}

async fn serves_epoch(client: &Client, conversation_id: ConvId, epoch: i32) -> bool {
    client
        .api
        .keys(conversation_id, None)
        .await
        .unwrap()
        .iter()
        .any(|key| key.epoch == epoch)
}

#[tokio::test]
#[ignore]
async fn a_new_device_reads_old_history_only_after_an_older_device_says_it_is_me() {
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
    assert_eq!(ann.send(view.id, "before the new device").await, Ok(1));
    bob.sync_until("bob reads it", |bob| {
        bob.texts(view.id) == ["before the new device"]
    })
    .await;
    stack.age_device(&bob.person, 73);
    bob.register().await.unwrap();

    let laptop = Client::up(&stack, &stack.device_of(&bob.person)).await;
    assert!(laptop.engine.status().history.locked);
    assert!(laptop.engine.status().history.unlocks_at.is_some());
    bob.sync_until("the older device is told about the new one", |bob| {
        bob.events
            .devices_added()
            .contains(&laptop.person.device_id)
    })
    .await;
    laptop
        .sync_until("the new device lists the chat", |laptop| {
            laptop.conversation(view.id).is_some()
        })
        .await;
    assert_eq!(ann.send(view.id, "after the new device").await, Ok(1));
    laptop
        .sync_until("the new device reads only what came after it", |laptop| {
            let texts = laptop.texts(view.id);
            texts.last().map(String::as_str) == Some("after the new device")
                && !texts.iter().any(|text| text == "before the new device")
        })
        .await;

    assert!(!reads_from_history(&laptop, view.id, "before the new device").await);
    assert!(!serves_epoch(&laptop, view.id, view.key_epoch).await);

    assert_eq!(
        laptop.api.confirm_device(bob.person.device_id).await,
        Err(ChatError::TooYoung)
    );
    bob.api
        .confirm_device(laptop.person.device_id)
        .await
        .expect("a seasoned device confirms the new one");
    laptop
        .sync_until("the history gate opens on the new device", |laptop| {
            !laptop.engine.status().history.locked
        })
        .await;
    assert!(serves_epoch(&laptop, view.id, view.key_epoch).await);
    assert!(reads_from_history(&laptop, view.id, "before the new device").await);
    assert_eq!(
        laptop.texts(view.id),
        ["before the new device", "after the new device"]
    );
}

#[tokio::test]
#[ignore]
async fn a_new_device_nobody_confirms_reads_old_history_after_seventy_two_hours() {
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
    assert_eq!(ann.send(view.id, "said long ago").await, Ok(1));
    bob.sync_until("bob reads it", |bob| {
        bob.texts(view.id) == ["said long ago"]
    })
    .await;

    let laptop = Client::up(&stack, &stack.device_of(&bob.person)).await;
    assert!(laptop.engine.status().history.locked);
    assert!(!reads_from_history(&laptop, view.id, "said long ago").await);

    stack.age_device(&laptop.person, 73);
    laptop.register().await.unwrap();
    laptop.engine.devices().await.unwrap();
    assert!(!laptop.engine.status().history.locked);
    assert!(serves_epoch(&laptop, view.id, view.key_epoch).await);
    laptop
        .sync_until("the old history opens on its own", |laptop| {
            laptop.texts(view.id) == ["said long ago"]
        })
        .await;
}
