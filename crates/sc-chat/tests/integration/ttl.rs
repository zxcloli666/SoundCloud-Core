use std::sync::Arc;
use std::time::Duration;

use sc_chat::search::Search;
use sc_chat::store::ExpiryTimer;
use sc_chat::{Clock, ConvId, Millis, SearchQuery, SystemKind, async_trait};

use crate::dm::direct;
use crate::support::client::Recorder;
use crate::support::{Client, Stack};

const HOUR_S: u32 = 3_600;
const SWEEP_WAIT: Duration = Duration::from_secs(90);

struct At(Millis);

#[async_trait]
impl Clock for At {
    fn now_ms(&self) -> Millis {
        self.0
    }

    async fn sleep(&self, _ms: u64) {}
}

fn found(client: &Client, conversation_id: ConvId, text: &str) -> bool {
    let query = SearchQuery {
        text: text.to_owned(),
        conversation_id: Some(conversation_id),
        limit: 10,
    };
    !Search::new(client.store().clone())
        .query(&query)
        .unwrap()
        .is_empty()
}

fn timer_rows(client: &Client, conversation_id: ConvId) -> Vec<Option<u32>> {
    client
        .systems(conversation_id)
        .iter()
        .filter(|event| event.kind == SystemKind::Ttl)
        .map(|event| event.ttl_s)
        .collect()
}

#[tokio::test]
#[ignore]
async fn a_dm_timer_expires_messages_on_the_server_and_on_devices_and_can_be_turned_off() {
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
    assert_eq!(ann.send(view.id, "kept forever").await, Ok(1));
    bob.sync_until("bob has the chat", |bob| {
        bob.texts(view.id) == ["kept forever"]
    })
    .await;

    let set = bob.api.set_ttl(view.id, Some(HOUR_S)).await.unwrap();
    assert_eq!(set.ttl_s, Some(HOUR_S));
    assert_eq!(ann.send(view.id, "vanishing soon").await, Ok(1));
    let sent = ann.message(view.id, "vanishing soon").unwrap();
    let expires_at = sent
        .expires_at
        .expect("the sender's copy carries the server timer");
    let in_an_hour = sent.ts + Millis::from(HOUR_S) * 1_000;
    assert!((expires_at - in_an_hour).abs() < 5_000);
    for client in [&ann, &bob] {
        client
            .sync_until("the timer row and the expiring post arrive", |client| {
                timer_rows(client, view.id) == [Some(HOUR_S)]
                    && client
                        .message(view.id, "vanishing soon")
                        .is_some_and(|message| message.expires_at == Some(expires_at))
            })
            .await;
        assert_eq!(
            client.message(view.id, "kept forever").unwrap().expires_at,
            None
        );
        assert_eq!(client.conversation(view.id).unwrap().ttl_s, Some(HOUR_S));
    }
    assert!(found(&bob, view.id, "vanishing"));

    let offline = ExpiryTimer::new(
        bob.store().clone(),
        Arc::new(At(expires_at + 1)),
        Arc::new(Recorder::default()),
    );
    offline.sweep().unwrap();
    assert!(bob.message(view.id, "vanishing soon").is_none());
    assert!(bob.message(view.id, "kept forever").is_some());
    assert!(!found(&bob, view.id, "vanishing"));

    let seq = sent.seq.unwrap();
    stack.sql(&format!(
        "UPDATE chat.messages SET expires_at = now() - interval '1 second' WHERE conversation_id = '{}' AND seq = {seq}",
        view.id
    ));
    ann.sync_within(
        "the server sweep expires the post and tells the sender",
        SWEEP_WAIT,
        |ann| ann.message(view.id, "vanishing soon").is_none(),
    )
    .await;
    assert!(
        ann.events
            .gone_messages()
            .iter()
            .any(|gone| gone.conversation_id == view.id && gone.seq == seq)
    );
    assert_eq!(
        stack.count(&format!(
            "SELECT count(*) FROM chat.messages WHERE conversation_id = '{}' AND seq = {seq}",
            view.id
        )),
        0
    );
    assert!(!found(&ann, view.id, "vanishing"));

    let off = ann.api.set_ttl(view.id, None).await.unwrap();
    assert_eq!(off.ttl_s, None);
    assert_eq!(ann.send(view.id, "back to normal").await, Ok(1));
    assert_eq!(
        ann.message(view.id, "back to normal").unwrap().expires_at,
        None
    );
    bob.sync_until("bob sees the timer off and the plain post", |bob| {
        timer_rows(bob, view.id) == [Some(HOUR_S), None]
            && bob
                .message(view.id, "back to normal")
                .is_some_and(|message| message.expires_at.is_none())
    })
    .await;
    assert_eq!(bob.texts(view.id), ["kept forever", "back to normal"]);
}
