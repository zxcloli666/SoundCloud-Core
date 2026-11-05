use sc_chat::ConversationKind;
use sc_chat::sync::{Local, SyncOutcome};
use serde_json::{Value, json};

use crate::sync_support::fixtures::{answer, entry, keys, post, summary, system, view};
use crate::sync_support::{self, PEER, rig};
use crate::{INC, ROOM, plan, synced};

#[tokio::test]
async fn a_room_chat_is_adopted_from_its_seat_and_asks_for_its_view_only_for_the_meta() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/messages"),
        json!({"messages": [post(ROOM, 6, 1, "seated")], "more": false}),
    );
    let mut room_view = view(ROOM, 6);
    room_view["kind"] = json!("room");
    room_view["peer"] = Value::Null;
    rig.server
        .json("GET", &format!("/chat/conversations/{ROOM}"), room_view);
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/keys"),
        keys(ROOM, &[1]),
    );
    let adopted = rig.engine.adopt_room(ROOM, "ROOM42", 5).await.unwrap();
    assert_eq!(adopted.kind, ConversationKind::Room);
    assert_eq!(adopted.room_code.as_deref(), Some("ROOM42"));
    assert_eq!(adopted.last_seq, 6);
    let history = &rig
        .server
        .calls_to("GET", &format!("/chat/conversations/{ROOM}/messages"))[0];
    assert!(history.query.contains("after=5"), "{}", history.query);

    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            3,
            INC,
            vec![entry(
                ROOM,
                7,
                vec![system(
                    7,
                    json!({"type": "join", "user_id": PEER, "name": "peer"}),
                )],
            )],
        ),
    );
    rig.engine.step(plan(&rig)).await.unwrap();
    assert_eq!(rig.local.summary(ROOM).unwrap().last_seq, 7);
    assert_eq!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{ROOM}"))
            .len(),
        1
    );
}

#[tokio::test]
async fn a_reset_keeps_room_chats_the_list_never_shows_and_catches_them_up() {
    let rig = rig();
    let mut room = summary(ROOM, 2);
    room.kind = ConversationKind::Room;
    room.room_code = Some("ROOM42".into());
    rig.local.put_conversations(&[room]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    for _ in 0..2 {
        rig.server.json(
            "GET",
            &format!("/chat/conversations/{ROOM}/messages"),
            json!({"messages": [post(ROOM, 3, 1, "while away")], "more": false}),
        );
    }
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{ROOM}/keys"),
        keys(ROOM, &[1]),
    );

    rig.engine.reconcile().await.unwrap();

    assert!(rig.events.gone().is_empty());
    assert_eq!(rig.local.summary(ROOM).unwrap().last_seq, 3);
    let shown = rig.events.messages().pop().unwrap();
    assert_eq!(shown.conversation_id, ROOM);
}

#[tokio::test]
async fn a_reset_purges_a_room_chat_whose_room_ended_while_away() {
    let rig = rig();
    let mut room = summary(ROOM, 2);
    room.kind = ConversationKind::Room;
    room.room_code = Some("ROOM42".into());
    rig.local.put_conversations(&[room]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    rig.server.on(
        "GET",
        &format!("/chat/conversations/{ROOM}/messages"),
        sync_support::Step::Json(403, json!({"error": "chat_not_member"})),
    );

    rig.engine.reconcile().await.unwrap();

    assert!(rig.local.summary(ROOM).is_none());
    assert!(
        rig.events
            .gone()
            .iter()
            .any(|gone| gone.conversations.contains(&ROOM))
    );
}
