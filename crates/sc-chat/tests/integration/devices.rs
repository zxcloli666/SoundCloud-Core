use sc_chat::ChatError;

use crate::support::{Client, Stack};

#[tokio::test]
#[ignore]
async fn a_young_device_cannot_drop_an_older_one_but_the_older_one_can_say_not_me() {
    let stack = Stack::from_env();
    let first = Client::up(&stack, &stack.person("eve")).await;
    let second = Client::up(&stack, &stack.device_of(&first.person)).await;

    let seen_by_first = first.engine.devices().await.unwrap();
    let newer = seen_by_first
        .iter()
        .find(|device| device.id == second.person.device_id)
        .unwrap();
    assert!(newer.actions.not_me && newer.actions.remove && !newer.actions.confirm);
    let seen_by_second = second.engine.devices().await.unwrap();
    let older = seen_by_second
        .iter()
        .find(|device| device.id == first.person.device_id)
        .unwrap();
    assert!(!older.actions.not_me && !older.actions.remove && !older.actions.confirm);

    let older_id = first.person.device_id;
    assert_eq!(
        second.api.remove_device(older_id).await,
        Err(ChatError::TooYoung)
    );
    assert_eq!(
        second.api.confirm_device(older_id).await,
        Err(ChatError::TooYoung)
    );
    assert_eq!(
        second.api.device_not_me(older_id).await,
        Err(ChatError::TooYoung)
    );
    first
        .api
        .device_not_me(second.person.device_id)
        .await
        .unwrap();
    assert_eq!(second.sync().await, Err(ChatError::DeviceRevoked));
    assert!(first.sync().await.is_ok());
}
