use sc_chat::Tier;
use sc_chat::sync::Local;
use serde_json::json;

use crate::sync_support::fixtures::{answer, entry, post, summary};
use crate::sync_support::{self, rig};
use crate::{A, INC, plan, synced};

#[tokio::test]
async fn a_cut_answer_polls_again_without_holding() {
    let rig = rig();
    synced(&rig);
    rig.engine.set_visible(true);
    rig.local.put_conversations(&[summary(A, 1)]).unwrap();
    rig.local.add_key(A, 1, sync_support::ck_bytes(A, 1));
    let mut cut = entry(A, 9, vec![post(A, 2, 1, "two")]);
    cut["more"] = json!(true);
    rig.server
        .json("POST", "/chat/sync", answer(2, INC, vec![cut]));

    rig.engine.step(plan(&rig)).await.unwrap();

    let next = rig.engine.plan(Tier::Direct);
    assert_eq!((next.hold_ms, next.delay_ms), (0, 0));
    assert_eq!(rig.local.summary(A).unwrap().last_seq, 2);
}

#[tokio::test]
async fn a_hidden_window_sends_no_focus() {
    let rig = rig();
    synced(&rig);
    rig.engine.set_focus(Some(A));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", sync_support::Step::Empty(204, vec![]));

    rig.engine.step(plan(&rig)).await.unwrap();
    rig.engine.set_visible(true);
    rig.engine.step(plan(&rig)).await.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["focus"], json!(null));
    assert_eq!(syncs[1].body["focus"], json!(A));
}
