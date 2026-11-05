mod room_support;
mod store_support;
mod sync_support;

use std::sync::Arc;
use std::time::Duration;

use room_support::{
    R, X, backdrop, carried_backdrop, conversation_gets, conversation_path, resolves, room_meta,
    room_view,
};
use sc_chat::sync::{Local, ROOM_META_ATTEMPTS_MAX, ROOM_META_TICK_MAX};
use sc_chat::{ChatError, ConversationKind, ConversationMeta, ConversationSummary};
use serde_json::json;
use sync_support::fixtures::{summary, view};
use sync_support::real::real;
use sync_support::{Step, rig};
use uuid::Uuid;

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
async fn a_room_read_without_a_meta_is_not_quiet_on_the_next_start() {
    let rig = real(&[quiet_room(R, 1)]);
    rig.server
        .json("GET", &conversation_path(), bare_room_view(R));

    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);

    assert!(rig.engine.store().quiet_rooms(32).unwrap().is_empty());
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
}

#[tokio::test]
async fn a_room_locked_by_a_missing_key_is_read_again_on_the_next_start() {
    let rig = real(&[quiet_room(R, 1)]);
    rig.server
        .json("GET", &format!("/chat/conversations/{R}/keys"), json!([]));
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );

    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);

    assert_eq!(rig.engine.store().quiet_rooms(32).unwrap(), vec![R]);
    assert_eq!(carried_backdrop(&rig), None);
}

#[tokio::test]
async fn the_sync_event_after_a_meta_read_carries_the_summary_stored_after_the_read() {
    let rig = real(&[quiet_room(R, 1)]);
    let store = Arc::clone(rig.engine.store());
    let mut renamed = quiet_room(R, 1);
    renamed.title = Some("Renamed meanwhile".into());
    rig.server.on(
        "GET",
        &conversation_path(),
        Step::Run(
            Box::new(move || store.put_conversations(&[renamed.clone()]).unwrap()),
            Box::new(Step::Json(200, bare_room_view(R))),
        ),
    );

    rig.engine.queue_quiet_rooms().unwrap();
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);

    let synced = rig.events.syncs();
    let sent = &synced.last().unwrap().conversations[0];
    assert_eq!(sent.id, R);
    assert_eq!(sent.title.as_deref(), Some("Renamed meanwhile"));
}

#[tokio::test]
async fn a_quiet_room_that_keeps_failing_backs_off_and_gives_up_for_this_start() {
    let rig = real(&[quiet_room(R, 1)]);
    for _ in 0..10 {
        rig.server
            .on("GET", &conversation_path(), Step::Fail(ChatError::Offline));
    }

    rig.engine.queue_quiet_rooms().unwrap();
    rig.engine.read_room_metas().await.unwrap();
    for (wait, gets) in [
        (30_000, 2),
        (60_000, 3),
        (59_000, 3),
        (61_000, 4),
        (240_000, 5),
        (3_600_000, 5),
    ] {
        rig.clock.advance(wait);
        rig.engine.read_room_metas().await.unwrap();
        assert_eq!(conversation_gets(&rig), gets, "after {wait} ms");
    }
    assert_eq!(conversation_gets(&rig), ROOM_META_ATTEMPTS_MAX as usize);
}

#[tokio::test]
async fn the_engine_polls_before_it_reads_a_quiet_room_meta() {
    let rig = rig();
    rig.local.put_conversations(&[quiet_room(R, 1)]).unwrap();
    rig.server.json("GET", "/chat/devices", json!([]));
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    rig.server
        .json("GET", &conversation_path(), bare_room_view(R));

    rig.engine.run(rig.clock.as_ref()).await.unwrap();

    let calls = rig.server.calls();
    let at = |path: &str| calls.iter().position(|call| call.path == path);
    let meta = at(&conversation_path()).expect("the quiet room is read");
    let poll = at("/chat/sync").expect("the engine polls");
    assert!(poll < meta, "{calls:?}");
}

#[tokio::test]
async fn a_wake_cuts_a_quiet_room_read_short_and_the_room_stays_due() {
    let rig = rig();
    rig.local.put_conversations(&[quiet_room(R, 1)]).unwrap();
    rig.server.json("GET", "/chat/devices", json!([]));
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    let engine = Arc::downgrade(&rig.engine);
    rig.server.on(
        "GET",
        &conversation_path(),
        Step::Stall(Box::new(move || {
            if let Some(engine) = engine.upgrade() {
                engine.kick();
            }
        })),
    );

    tokio::time::timeout(Duration::from_secs(5), rig.engine.run(rig.clock.as_ref()))
        .await
        .expect("a wake ends the stalled read")
        .unwrap();

    rig.server
        .json("GET", &conversation_path(), bare_room_view(R));
    assert_eq!(rig.engine.read_room_metas().await.unwrap(), 1);
    let gets = rig.server.calls_to("GET", &conversation_path());
    assert_eq!(gets.len(), 2);
}
