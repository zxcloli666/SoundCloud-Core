mod sync_support;

use sc_chat::store::{StoredContent, SyncState};
use sc_chat::sync::{Local, SyncOutcome};
use sc_chat::{DEVICE_AGE_RULE_MS, MessagesOrigin, Tier};
use serde_json::json;
use sync_support::fixtures::{answer, device, entry, keys, post, summary};
use sync_support::{DEVICE, START_MS, ck_bytes, rig};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);
const OTHER: Uuid = Uuid::from_u128(0xd9);

fn synced(rig: &sync_support::Rig) {
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
}

#[tokio::test]
async fn a_message_on_a_new_epoch_fetches_its_key_first() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, ck_bytes(A, 1));
    let keys_path = format!("/chat/conversations/{A}/keys");
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![post(A, 2, 2, "rekeyed")])]),
    );
    rig.server.json("GET", &keys_path, keys(A, &[2]));

    assert_eq!(
        rig.engine.step(rig.engine.plan(Tier::Direct)).await,
        Ok(SyncOutcome::Changed)
    );

    assert_eq!(
        rig.server.calls_to("GET", &keys_path)[0].query,
        "from_epoch=2"
    );
    assert!(rig.local.has_key(A, 2));
    assert!(matches!(
        rig.local.stored(A)[0].content,
        StoredContent::Open(_)
    ));
}

#[tokio::test]
async fn locked_history_stays_sealed_until_the_gate_opens() {
    let rig = rig();
    synced(&rig);
    let created = START_MS - 3_600_000;
    rig.server.json(
        "GET",
        "/chat/devices",
        json!([device(DEVICE, created, false, true)]),
    );
    let devices = rig.engine.devices().await.unwrap();
    assert!(devices[0].current);
    let gate = rig.engine.status().history;
    assert!(gate.locked);
    assert_eq!(gate.unlocks_at, Some(created + DEVICE_AGE_RULE_MS));

    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    let keys_path = format!("/chat/conversations/{A}/keys");
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![post(A, 1, 0, "old")])]),
    );
    rig.server.json("GET", &keys_path, json!([]));
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(3, INC, vec![entry(A, 2, vec![post(A, 2, 0, "older")])]),
    );
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();

    assert_eq!(rig.server.calls_to("GET", &keys_path).len(), 1);
    assert!(
        rig.local
            .stored(A)
            .iter()
            .all(|message| matches!(message.content, StoredContent::Sealed(_)))
    );

    let mut unlocked = answer(4, INC, vec![]);
    unlocked["devices"] = json!([device(DEVICE, created, true, true)]);
    rig.server.json("POST", "/chat/sync", unlocked);
    rig.server.json("GET", &keys_path, keys(A, &[0]));
    rig.events.clear();
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();

    assert!(!rig.engine.status().history.locked);
    assert_eq!(
        rig.server.calls_to("GET", &keys_path)[1].query,
        "from_epoch=0"
    );
    assert!(
        rig.local
            .stored(A)
            .iter()
            .all(|message| matches!(message.content, StoredContent::Open(_)))
    );
    let reopened = &rig.events.messages()[0];
    assert_eq!(reopened.origin, MessagesOrigin::History);
    assert_eq!(reopened.messages.len(), 2);
}

#[tokio::test]
async fn the_gate_opens_by_itself_after_seventy_two_hours() {
    let rig = rig();
    synced(&rig);
    let created = START_MS - 3_600_000;
    rig.server.json(
        "GET",
        "/chat/devices",
        json!([device(DEVICE, created, false, true)]),
    );
    rig.engine.devices().await.unwrap();
    assert!(rig.engine.status().history.locked);

    rig.clock.advance(DEVICE_AGE_RULE_MS as u64);
    rig.server.json(
        "GET",
        "/chat/devices",
        json!([device(DEVICE, created, false, true)]),
    );
    rig.engine.devices().await.unwrap();

    assert!(!rig.engine.status().history.locked);
}

#[tokio::test]
async fn a_new_device_alerts_older_devices_once() {
    let rig = rig();
    synced(&rig);
    let mine = START_MS - 10 * DEVICE_AGE_RULE_MS;
    let list = json!([
        device(DEVICE, mine, true, true),
        device(OTHER, START_MS - 60_000, false, false),
    ]);
    let mut first = answer(2, INC, vec![]);
    first["devices"] = list.clone();
    let mut again = answer(3, INC, vec![]);
    again["devices"] = list;
    rig.server.json("POST", "/chat/sync", first);
    rig.server.json("POST", "/chat/sync", again);

    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();

    let alerts = rig.events.devices();
    assert_eq!(alerts.len(), 1);
    let added = &alerts[0].device;
    assert_eq!(added.id, OTHER);
    assert!(added.actions.confirm && added.actions.not_me && added.actions.remove);
    assert!(!rig.engine.status().history.locked);
}

#[tokio::test]
async fn a_young_device_cannot_act_on_older_ones() {
    let rig = rig();
    rig.server.json(
        "GET",
        "/chat/devices",
        json!([
            device(DEVICE, START_MS - 60_000, false, true),
            device(OTHER, START_MS - 10 * DEVICE_AGE_RULE_MS, true, false),
        ]),
    );
    let devices = rig.engine.devices().await.unwrap();
    let older = devices.iter().find(|device| device.id == OTHER).unwrap();
    assert!(!older.actions.confirm && !older.actions.not_me && !older.actions.remove);
    let me = devices.iter().find(|device| device.current).unwrap();
    assert!(me.actions.remove);
    assert!(rig.events.devices().is_empty());
}
