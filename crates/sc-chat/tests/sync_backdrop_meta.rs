mod room_support;
mod store_support;
mod sync_support;

use room_support::{
    R, X, Y, backdrop, carried_backdrop, conversation_gets, conversation_path, joined, keys_gets,
    live, meta_message, resolves, room_meta, room_view,
};
use sc_chat::ConversationKind;
use serde_json::json;
use sync_support::fixtures::{post, summary};
use sync_support::real::real;
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
