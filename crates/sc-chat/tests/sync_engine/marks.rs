use sc_chat::ConversationKind;
use sc_chat::sync::{Local, SyncOutcome};
use serde_json::json;

use crate::sync_support::fixtures::{answer, card, entry, post, summary, view};
use crate::sync_support::{self, ME, PEER, START_MS, rig};
use crate::{A, B, C, INC, plan, synced};

#[tokio::test]
async fn read_marks_ride_the_next_sync_once() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 5)]).unwrap();
    rig.engine.queue_read(A, 5);
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Idle));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Idle));

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["read"], json!([[A, 5]]));
    assert_eq!(syncs[1].body["read"], json!([]));
    assert_eq!(syncs[0].body["known"], json!([[A, 5]]));
    assert_eq!(rig.local.summary(A).unwrap().last_read_seq, 5);
}

#[tokio::test]
async fn received_messages_are_marked_delivered_and_typing_reaches_the_focus() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    rig.engine.set_visible(true);
    rig.engine.set_focus(Some(A));
    let mut live = entry(A, 2, vec![post(A, 2, 1, "hey")]);
    live["typing"] = json!([PEER]);
    live["marks"] = json!([{"user_id": PEER, "read_seq": 1, "delivered_seq": 2}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![live]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["focus"], json!(A));
    assert_eq!(syncs[1].body["delivered"], json!([[A, 2]]));
    assert_eq!(rig.events.typing()[0].users, vec![PEER.to_owned()]);
    assert_eq!(rig.local.marks_of(A)[0].delivered_seq, 2);
    let marks = &rig.events.syncs()[0].marks[0];
    assert_eq!(marks.conversation_id, A);
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}"))
            .is_empty()
    );
}

#[tokio::test]
async fn the_badge_counts_loud_unread_chats_and_every_mention() {
    let rig = rig();
    synced(&rig);
    let mut loud = summary(A, 1);
    loud.unread = 1;
    let mut muted = summary(B, 3);
    muted.unread = 3;
    muted.mentions = 1;
    muted.muted_until = Some(START_MS + 3_600_000);
    let mut room = summary(C, 2);
    room.kind = ConversationKind::Room;
    room.unread = 2;
    rig.local.put_conversations(&[loud, muted, room]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));

    rig.engine.load_tally().unwrap();
    let status = rig.engine.status();
    assert_eq!((status.unread, status.mentions), (1, 1));

    rig.engine.queue_read(A, 1);
    assert_eq!(rig.engine.status().unread, 0);

    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![post(A, 2, 1, "again")])]),
    );
    rig.engine.step(plan(&rig)).await.unwrap();
    assert_eq!(rig.engine.status().unread, 1);

    let mut gone = answer(3, INC, vec![]);
    gone["gone"] = json!([A, B]);
    rig.server.json("POST", "/chat/sync", gone);
    rig.engine.step(plan(&rig)).await.unwrap();
    let status = rig.engine.status();
    assert_eq!((status.unread, status.mentions), (0, 0));
}

#[tokio::test]
async fn a_bump_that_carries_only_my_settled_mark_refreshes_the_chat() {
    let rig = rig();
    synced(&rig);
    let mut invited = summary(A, 3);
    invited.accepted = false;
    invited.last_read_seq = 3;
    rig.local.put_conversations(&[invited]).unwrap();
    let mut bump = entry(A, 3, vec![]);
    bump["marks"] = json!([{"user_id": ME, "read_seq": 3, "delivered_seq": 3}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![bump]));
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 3));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    assert!(rig.local.summary(A).unwrap().accepted);
}

#[tokio::test]
async fn my_read_mark_from_another_device_moves_without_a_refresh() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 5)]).unwrap();
    let mut read = entry(A, 5, vec![]);
    read["marks"] = json!([{"user_id": ME, "read_seq": 5, "delivered_seq": 5}]);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![read]));

    rig.engine.step(plan(&rig)).await.unwrap();

    assert_eq!(rig.local.summary(A).unwrap().last_read_seq, 5);
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}"))
            .is_empty()
    );
}
