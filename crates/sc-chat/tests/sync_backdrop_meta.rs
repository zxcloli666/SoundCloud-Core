mod room_support;
mod store_support;
mod sync_support;

use room_support::{
    R, X, Y, backdrop, carried_backdrop, conversation_gets, conversation_path, joined, keys_gets,
    live, meta_message, resolves, room_meta, room_view,
};
use sc_chat::sync::{Local, ROOM_META_TICK_MAX};
use sc_chat::{ConversationKind, ConversationMeta, ConversationSummary};
use serde_json::json;
use sync_support::fixtures::{post, summary, view};
use sync_support::real::real;
use sync_support::rig;
use uuid::Uuid;

const ELSEWHERE: Uuid = Uuid::from_u128(0x7f);

#[tokio::test]
async fn a_meta_answer_for_another_room_is_never_stored() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;

    let mut stray = room_view(Some(&room_meta(Some(Y))), 1);
    stray["id"] = json!(ELSEWHERE);
    rig.server.json("GET", &conversation_path(), stray);
    live(&rig, 2, 5, vec![meta_message(5, &room_meta(Some(Y)))]).await;

    assert_eq!(conversation_gets(&rig), 2);
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
    assert_eq!(resolves(&rig, Y), None);
    assert_eq!(rig.engine.store().room_backdrop(ELSEWHERE).unwrap(), None);
    let elsewhere_keys = format!("/chat/conversations/{ELSEWHERE}/keys");
    assert!(rig.server.calls_to("GET", &elsewhere_keys).is_empty());

    live(&rig, 3, 6, vec![post(R, 6, 1, "chat")]).await;
    assert_eq!(conversation_gets(&rig), 2);
}

#[tokio::test]
async fn a_meta_locked_by_a_missing_key_opens_once_the_key_arrives() {
    let rig = real(&[]);
    rig.server
        .json("GET", &format!("/chat/conversations/{R}/keys"), json!([]));
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;
    assert_eq!(carried_backdrop(&rig), None);

    rig.serve_keys(R, &[1]);
    assert_eq!(rig.engine.refresh_keys(R).await.unwrap(), vec![1]);
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );
    live(&rig, 2, 5, vec![post(R, 5, 1, "chat")]).await;

    assert_eq!((conversation_gets(&rig), keys_gets(&rig)), (2, 2));
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
    let synced = rig.events.syncs();
    assert!(
        synced
            .last()
            .unwrap()
            .conversations
            .iter()
            .any(|summary| summary.id == R)
    );

    live(&rig, 3, 6, vec![post(R, 6, 1, "more")]).await;
    assert_eq!(conversation_gets(&rig), 2);
}

#[tokio::test]
async fn a_known_room_reads_its_meta_once_after_a_start() {
    let mut seat = summary(R, 4);
    seat.kind = ConversationKind::Room;
    seat.room_code = Some("ROOM42".into());
    let rig = real(&[seat]);
    rig.serve_keys(R, &[1]);
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );

    for (pos, seq) in [(2, 5), (3, 6), (4, 7)] {
        live(&rig, pos, seq, vec![post(R, seq, 1, "chat")]).await;
    }

    assert_eq!(conversation_gets(&rig), 1);
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
}

fn quiet_room(id: Uuid, last_at: i64) -> ConversationSummary {
    let mut seat = summary(id, 4);
    seat.kind = ConversationKind::Room;
    seat.room_code = Some("ROOM42".into());
    seat.last_at = last_at;
    seat
}

fn bare_room_view(id: Uuid) -> serde_json::Value {
    let mut bare = view(id, 0);
    bare["kind"] = json!("room");
    bare["peer"] = serde_json::Value::Null;
    bare
}

#[tokio::test]
async fn an_upgraded_store_reads_the_meta_of_a_quiet_room_after_start() {
    let rig = real(&[quiet_room(R, 1)]);
    rig.serve_keys(R, &[1]);
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );

    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);

    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
    let synced = rig.events.syncs();
    assert_eq!(synced.last().unwrap().conversations[0].id, R);
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 0);
    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 0);
    assert_eq!(conversation_gets(&rig), 1);
}

#[tokio::test]
async fn quiet_rooms_are_read_a_few_per_tick_and_a_room_with_a_meta_is_left_alone() {
    let quiet: Vec<Uuid> = (1..=3).map(|n| Uuid::from_u128(0x7e00 + n)).collect();
    let kept = Uuid::from_u128(0x7e10);
    let mut seats: Vec<ConversationSummary> = quiet
        .iter()
        .zip(1..)
        .map(|(id, at)| quiet_room(*id, at))
        .collect();
    seats.push(quiet_room(kept, 9));
    let rig = real(&seats);
    rig.engine
        .store()
        .put_room_meta(kept, Some(&ConversationMeta::default()))
        .unwrap();
    for id in &quiet {
        rig.server.json(
            "GET",
            &format!("/chat/conversations/{id}"),
            bare_room_view(*id),
        );
    }

    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(
        rig.engine.read_room_metas().await.unwrap(),
        ROOM_META_TICK_MAX
    );
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 0);

    for id in &quiet {
        let gets = rig
            .server
            .calls_to("GET", &format!("/chat/conversations/{id}"));
        assert_eq!(gets.len(), 1, "{id}");
    }
    let untouched = format!("/chat/conversations/{kept}");
    assert!(rig.server.calls_to("GET", &untouched).is_empty());
}

#[tokio::test]
async fn the_engine_reads_a_quiet_room_meta_before_its_first_poll() {
    let rig = rig();
    rig.local.put_conversations(&[quiet_room(R, 1)]).unwrap();
    rig.server.json("GET", "/chat/devices", json!([]));
    rig.server
        .json("GET", &conversation_path(), bare_room_view(R));

    rig.engine.run(rig.clock.as_ref()).await.unwrap();

    let calls = rig.server.calls();
    let at = |path: &str| calls.iter().position(|call| call.path == path);
    let meta = at(&conversation_path()).unwrap();
    assert!(at("/chat/sync").is_none_or(|poll| meta < poll), "{calls:?}");
}
