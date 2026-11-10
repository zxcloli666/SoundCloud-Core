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
async fn a_bump_that_repeats_a_peer_mark_refreshes_the_chat_and_a_moved_mark_does_not() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 3)]).unwrap();
    let path = format!("/chat/conversations/{A}");
    let marked = |read: i64| {
        let mut bump = entry(A, 3, vec![]);
        bump["marks"] = json!([{"user_id": PEER, "read_seq": read, "delivered_seq": 3}]);
        bump
    };
    for (pos, read) in [(2, 2), (3, 3)] {
        rig.server
            .json("POST", "/chat/sync", answer(pos, INC, vec![marked(read)]));
        rig.engine.step(plan(&rig)).await.unwrap();
    }
    assert!(rig.server.calls_to("GET", &path).is_empty());

    let mut timed = view(A, 3);
    timed["ttl_s"] = json!(3_600);
    rig.server
        .json("POST", "/chat/sync", answer(4, INC, vec![marked(3)]));
    rig.server.json("GET", &path, timed);
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    assert_eq!(rig.server.calls_to("GET", &path).len(), 1);
    assert_eq!(rig.local.summary(A).unwrap().ttl_s, Some(3_600));
}

#[tokio::test]
async fn a_view_rev_rereads_the_chat_once_per_value_even_under_messages() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    let path = format!("/chat/conversations/{A}");
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    let mut timed = view(A, 4);
    timed["ttl_s"] = json!(3_600);
    rig.server.json("GET", &path, view(A, 2));
    rig.server.json("GET", &path, timed);
    for (seq, rev, reads) in [(2, 11, 1), (3, 11, 1), (4, 12, 2)] {
        let mut live = entry(A, seq, vec![post(A, seq, 1, "hey")]);
        live["rev"] = json!(rev);
        rig.server
            .json("POST", "/chat/sync", answer(seq as u64, INC, vec![live]));
        rig.engine.step(plan(&rig)).await.unwrap();
        assert_eq!(rig.server.calls_to("GET", &path).len(), reads);
    }
    assert_eq!(rig.local.summary(A).unwrap().ttl_s, Some(3_600));
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
