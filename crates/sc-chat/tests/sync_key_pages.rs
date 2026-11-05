mod sync_support;

use std::time::Duration;

use sc_chat::store::{StoredContent, SyncState};
use sc_chat::sync::Local;
use sc_chat::{ChatError, Tier};
use serde_json::json;
use sync_support::fixtures::{answer, entry, keys, post, summary};
use sync_support::{Rig, Step, rig};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);

fn keys_path() -> String {
    format!("/chat/conversations/{A}/keys")
}

fn all_open(rig: &Rig) -> bool {
    rig.local
        .stored(A)
        .iter()
        .all(|message| matches!(message.content, StoredContent::Open(_)))
}

async fn a_sealed_post() -> Rig {
    let rig = rig();
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(A, 1, vec![post(A, 1, 0, "old")])]),
    );
    rig.server.json("GET", &keys_path(), json!([]));
    rig.engine
        .step(rig.engine.plan(Tier::Direct))
        .await
        .unwrap();
    assert!(!all_open(&rig));
    rig
}

fn full_first_page(rig: &Rig) {
    let epochs: Vec<u32> = (0..64).collect();
    rig.server.json("GET", &keys_path(), keys(A, &epochs));
}

#[tokio::test]
async fn keys_from_a_first_page_open_old_messages_even_when_a_later_page_fails() {
    let rig = a_sealed_post().await;
    full_first_page(&rig);
    rig.server
        .on("GET", &keys_path(), Step::Fail(ChatError::Offline));

    assert_eq!(rig.engine.refresh_keys(A).await, Err(ChatError::Offline));

    assert!(rig.local.has_key(A, 0));
    assert!(all_open(&rig));
}

#[tokio::test]
async fn a_key_fetch_cut_between_pages_installs_nothing_and_the_next_fetch_opens_the_messages() {
    let rig = a_sealed_post().await;
    full_first_page(&rig);
    rig.server
        .on("GET", &keys_path(), Step::Stall(Box::new(|| {})));

    let cut = tokio::time::timeout(Duration::from_millis(200), rig.engine.refresh_keys(A)).await;

    assert!(cut.is_err());
    assert!(!rig.local.has_key(A, 0));
    rig.server.json("GET", &keys_path(), keys(A, &[0]));
    assert_eq!(rig.engine.refresh_keys(A).await, Ok(vec![0]));
    assert!(all_open(&rig));
}
