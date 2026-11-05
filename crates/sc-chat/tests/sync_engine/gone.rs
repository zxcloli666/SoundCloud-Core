use sc_chat::sync::{Local, SyncOutcome};
use sc_chat::{GoneMessage, MessagesOrigin};
use serde_json::json;

use crate::sync_support::fixtures::{answer, entry, post, summary};
use crate::sync_support::{self, START_MS, rig};
use crate::{A, B, INC, open, plan, synced};

#[tokio::test]
async fn gone_purges_conversations_and_expired_messages() {
    let rig = rig();
    synced(&rig);
    rig.local
        .put_conversations(&[summary(A, 3), summary(B, 1)])
        .unwrap();
    rig.local
        .put_messages(&[
            open(A, 1, "one"),
            open(A, 2, "two"),
            open(A, 3, "three"),
            open(B, 1, "b"),
        ])
        .unwrap();
    let mut gone = answer(2, INC, vec![]);
    gone["gone"] = json!([B]);
    gone["gone_msgs"] = json!([[A, 3]]);
    rig.server.json("POST", "/chat/sync", gone);

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    assert_eq!(rig.local.summary(B), None);
    assert!(rig.local.stored(B).is_empty());
    let left: Vec<i64> = rig
        .local
        .stored(A)
        .iter()
        .map(|message| message.seq)
        .collect();
    assert_eq!(left, vec![1, 2]);
    let event = &rig.events.gone()[0];
    assert_eq!(event.conversations, vec![B]);
    assert_eq!(
        event.messages,
        vec![GoneMessage {
            conversation_id: A,
            seq: 3
        }]
    );
    assert_eq!(rig.local.summary(A).unwrap().preview.unwrap().seq, 2);
    assert_eq!(rig.local.sync_state().unwrap().pos, 2);
}

#[tokio::test]
async fn expiring_messages_keep_their_deadline() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    let mut expiring = post(A, 2, 1, "soon gone");
    expiring["expires_at"] = json!(START_MS + 3_600_000);
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 2, vec![expiring])]),
    );

    assert_eq!(rig.engine.step(plan(&rig)).await, Ok(SyncOutcome::Changed));

    let stored = rig.local.stored(A);
    assert_eq!(stored[0].expires_at, Some(START_MS + 3_600_000));
    let event = &rig.events.messages()[0];
    assert_eq!(event.origin, MessagesOrigin::Sync);
    assert_eq!(event.messages[0].expires_at, Some(START_MS + 3_600_000));
}
