use sc_chat::Tier;

use crate::dm::direct;
use crate::support::{Client, Stack};

const DEAD: &str = "http://127.0.0.1:9";

#[tokio::test]
#[ignore]
async fn a_dead_direct_route_falls_back_to_the_podman_relay_and_the_relay_leads_after() {
    let stack = Stack::from_env();
    let bob = Client::up(&stack, &stack.person("bob")).await;
    let person = stack.person("gus");
    stack.follow(&bob.person, &person);
    let gus = Client::on(
        vec![
            (Tier::Direct, DEAD.into()),
            (Tier::Relay, stack.relay.clone()),
        ],
        &person,
    );
    assert!(gus.api.config().await.unwrap().enabled);
    gus.register().await.unwrap();
    assert_eq!(gus.api.tier_order().first(), Some(&Tier::Relay));
    let tiers: Vec<Tier> = gus.transport.sent().iter().map(|(tier, _)| *tier).collect();
    assert_eq!(tiers, [Tier::Direct, Tier::Relay, Tier::Relay]);
    gus.sync_until("the sync falls through to the relay", |gus| {
        gus.engine.status().tier == Some(Tier::Relay)
    })
    .await;

    let view = gus
        .api
        .create_conversation(&direct(&bob.person.user_id))
        .await
        .unwrap();
    gus.engine.adopt(&view).await.unwrap();
    assert_eq!(gus.send(view.id, "through the relay").await, Ok(1));
    bob.sync_until("bob reads what came through the relay", |bob| {
        bob.texts(view.id) == ["through the relay"]
    })
    .await;
    assert_eq!(bob.send(view.id, "back over direct").await, Ok(1));
    gus.sync_until("the answer reaches gus over the relay", |gus| {
        gus.texts(view.id) == ["through the relay", "back over direct"]
    })
    .await;
    assert!(
        gus.transport
            .sent()
            .iter()
            .any(|(tier, _)| *tier == Tier::Relay)
    );
}
