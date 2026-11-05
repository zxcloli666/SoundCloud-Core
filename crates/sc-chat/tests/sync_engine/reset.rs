use sc_chat::store::{StoredContent, SyncState};
use sc_chat::sync::{Local, SyncOutcome};
use sc_chat::{MessageBody, PreviewKind};
use serde_json::json;
use uuid::Uuid;

use crate::sync_support::fixtures::{answer, card, entry, keys, post, summary, view};
use crate::sync_support::{self, PEER, rig};
use crate::{A, B, INC, open, plan, synced};

#[tokio::test]
async fn first_sync_resets_and_reconciles_the_list() {
    let rig = rig();
    rig.local.put_conversations(&[summary(B, 4)]).unwrap();
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    assert!(rig.engine.reset_pending());

    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 3)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two"), post(A, 3, 1, "three")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.engine.reconcile().await.unwrap();

    assert!(!rig.engine.reset_pending());
    assert_eq!(rig.local.summary(B), None);
    assert_eq!(rig.events.gone()[0].conversations, vec![B]);
    let a = rig.local.summary(A).unwrap();
    assert_eq!(a.last_seq, 3);
    assert_eq!(a.unread, 3);
    assert_eq!(a.peer.unwrap().name, "peer");
    let preview = a.preview.unwrap();
    assert_eq!(
        (preview.seq, preview.kind, preview.text.as_str()),
        (3, PreviewKind::Text, "three")
    );
    assert_eq!(
        rig.local.sync_state().unwrap(),
        SyncState {
            pos: 5,
            inc: Some(INC)
        }
    );
    let reset = rig
        .events
        .syncs()
        .into_iter()
        .find(|sync| sync.reset)
        .unwrap();
    assert_eq!(reset.conversations.len(), 1);
    let history = &rig
        .server
        .calls_to("GET", &format!("/chat/conversations/{A}/messages"))[0];
    assert_eq!(history.query, "limit=50");
    assert!(
        rig.local
            .stored(A)
            .iter()
            .all(|message| matches!(message.content, StoredContent::Open(_)))
    );
}

#[tokio::test]
async fn incarnation_change_keeps_local_rows_above_the_server() {
    let rig = rig();
    rig.local
        .set_sync_state(SyncState {
            pos: 40,
            inc: Some(Uuid::from_u128(0x01d)),
        })
        .unwrap();
    rig.local.put_conversations(&[summary(A, 10)]).unwrap();
    rig.local
        .put_messages(&[open(A, 8, "eight"), open(A, 9, "nine"), open(A, 10, "ten")])
        .unwrap();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(3, INC, vec![entry(A, 8, vec![])]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    assert_eq!(rig.local.stored(A).len(), 3);

    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 8)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.engine.reconcile().await.unwrap();

    assert_eq!(rig.local.kept_above(), vec![(A, 8)]);
    let local_only: Vec<bool> = rig
        .local
        .stored(A)
        .iter()
        .map(|message| message.local_only)
        .collect();
    assert_eq!(local_only.iter().filter(|kept| **kept).count(), 2);
    assert_eq!(
        rig.local.sync_state().unwrap(),
        SyncState {
            pos: 3,
            inc: Some(INC)
        }
    );
    assert!(
        rig.server
            .calls_to("GET", &format!("/chat/conversations/{A}/messages"))
            .is_empty()
    );
}

#[tokio::test]
async fn an_unknown_conversation_loads_its_view_and_newest_page() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![])]),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    let a = rig.local.summary(A).unwrap();
    assert_eq!((a.last_seq, a.unread), (2, 2));
    let event = &rig.events.messages()[0];
    assert_eq!(event.messages.len(), 2);
    assert!(matches!(&event.messages[1].body, MessageBody::Text(rich) if rich.text == "two"));
    let sync = rig.events.syncs().pop().unwrap();
    assert_eq!(sync.conversations[0].id, A);
}

struct SavedProbe {
    local: std::sync::Arc<sync_support::MemoryLocal>,
    saved: std::sync::Mutex<Vec<bool>>,
}

impl sc_chat::Events for SavedProbe {
    fn emit(&self, event: sc_chat::CoreEvent) {
        if let sc_chat::CoreEvent::Messages(shown) = event {
            let saved = self.local.summary(shown.conversation_id).is_some();
            self.saved.lock().unwrap().push(saved);
        }
    }
}

#[tokio::test]
async fn a_reset_shows_messages_of_a_new_chat_only_after_the_chat_is_saved() {
    let rig = rig();
    let probe = std::sync::Arc::new(SavedProbe {
        local: rig.local.clone(),
        saved: std::sync::Mutex::default(),
    });
    let engine = sc_chat::sync::SyncEngine::with_parts(
        rig.engine.api().clone(),
        rig.local.clone(),
        probe.clone(),
        std::sync::Arc::new(sync_support::TestCipher),
    );
    rig.server
        .json("POST", "/chat/sync", answer(5, INC, vec![]));
    assert_eq!(engine.step(plan(&rig)).await, Ok(SyncOutcome::Reset));
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [view(A, 1)], "next_cursor": null}),
    );
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );

    engine.reconcile().await.unwrap();

    assert_eq!(*probe.saved.lock().unwrap(), [true]);
}

#[tokio::test]
async fn a_new_chat_caught_up_from_history_is_marked_delivered() {
    let rig = rig();
    synced(&rig);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![])]),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/messages"),
        json!({"messages": [post(A, 1, 1, "one"), post(A, 2, 1, "two")], "more": false}),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{A}/keys"),
        keys(A, &[1]),
    );
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[1].body["delivered"], json!([[A, 2]]));
}
