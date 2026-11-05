mod room_support;
mod store_support;
mod sync_support;

use room_support::{
    R, X, Y, backdrop, carried_backdrop, conversation_gets, conversation_path, joined, live,
    meta_message, resolves, room_meta, room_view,
};
use sc_chat::sync::ROOM_META_RETRY_MS;
use sc_chat::sync::SyncOutcome;
use sc_chat::{ChatError, ConversationKind};
use serde_json::json;
use sync_support::Step;
use sync_support::fixtures::{answer, post, summary};
use sync_support::real::{INC, direct_plan, real};
use uuid::Uuid;

#[tokio::test]
async fn a_dropped_meta_fetch_backs_off_and_then_heals() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{R}/messages"),
        json!({"messages": [], "more": false}),
    );
    rig.server
        .on("GET", &conversation_path(), Step::Fail(ChatError::Offline));
    rig.engine.adopt_room(R, "ROOM42", 4).await.unwrap();
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);

    for (pos, seq) in [(2, 5), (3, 6)] {
        live(&rig, pos, seq, vec![post(R, seq, 1, "busy room")]).await;
    }
    assert_eq!(conversation_gets(&rig), 1);

    rig.clock.advance(ROOM_META_RETRY_MS as u64);
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );
    live(&rig, 4, 7, vec![post(R, 7, 1, "back online")]).await;
    assert_eq!(conversation_gets(&rig), 2);
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));

    rig.clock.advance(ROOM_META_RETRY_MS as u64);
    live(&rig, 5, 8, vec![post(R, 8, 1, "again")]).await;
    assert_eq!(conversation_gets(&rig), 2);
}

#[tokio::test]
async fn a_busy_server_is_asked_again_only_after_its_retry() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(None, 1)).await;
    rig.server.on(
        "GET",
        &conversation_path(),
        Step::Fail(ChatError::Busy { retry_ms: 90_000 }),
    );
    live(&rig, 2, 5, vec![meta_message(5, &room_meta(Some(X)))]).await;
    assert_eq!(conversation_gets(&rig), 2);

    rig.clock.advance(60_000);
    live(&rig, 3, 6, vec![post(R, 6, 1, "waiting")]).await;
    assert_eq!(conversation_gets(&rig), 2);

    rig.clock.advance(30_000);
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );
    live(&rig, 4, 7, vec![post(R, 7, 1, "now")]).await;
    assert_eq!(conversation_gets(&rig), 3);
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
}

#[tokio::test]
async fn a_meta_caught_up_after_a_reset_is_fetched_once() {
    let mut seat = summary(R, 4);
    seat.kind = ConversationKind::Room;
    seat.room_code = Some("ROOM42".into());
    let rig = real(&[seat]);
    rig.serve_keys(R, &[1]);
    let mut reset = answer(9, INC, vec![]);
    reset["reset"] = json!(true);
    rig.server.json("POST", "/chat/sync", reset);
    assert_eq!(rig.engine.step(direct_plan()).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    let away = json!({
        "messages": [meta_message(5, &room_meta(Some(Y))), meta_message(6, &room_meta(Some(X)))],
        "more": false,
    });
    for _ in 0..2 {
        rig.server.json(
            "GET",
            &format!("/chat/conversations/{R}/messages"),
            away.clone(),
        );
    }
    rig.server.json(
        "GET",
        &conversation_path(),
        room_view(Some(&room_meta(Some(X))), 1),
    );

    rig.engine.reconcile().await.unwrap();

    assert_eq!(conversation_gets(&rig), 1);
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));
    assert_eq!(resolves(&rig, Y), None);
    assert_eq!(carried_backdrop(&rig), Some(Uuid::from_u128(X)));
}

#[tokio::test]
async fn a_room_without_any_meta_drops_a_stale_backdrop() {
    let rig = real(&[]);
    rig.serve_keys(R, &[1]);
    joined(&rig, room_view(Some(&room_meta(Some(X))), 1)).await;
    assert_eq!(resolves(&rig, X), Some(backdrop(X)));

    joined(&rig, room_view(None, 1)).await;
    assert_eq!(rig.engine.store().room_backdrop(R).unwrap(), None);
    assert_eq!(resolves(&rig, X), None);
    assert_eq!(carried_backdrop(&rig), None);
}
