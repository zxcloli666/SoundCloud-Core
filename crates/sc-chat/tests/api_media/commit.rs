use std::sync::Arc;

use sc_chat::ChatError;
use sc_chat::media::queue;
use sc_chat::store::{Store, UploadObject, UploadState};
use serde_json::{Value, json};
use uuid::Uuid;

use super::sync_support::{START_MS, Step, rig};
use super::{IMAGE, QUOTA, THUMB, queued, reserve_answer};

fn uploaded(store: &Store, id: Uuid) {
    for object in [UploadObject::Image, UploadObject::Thumb] {
        store.upload_object_done(id, object, START_MS).unwrap();
    }
}

fn committed() -> Value {
    json!({"id": IMAGE, "bytes": 176_128, "used": 176_128, "quota": QUOTA})
}

fn removed_during_commit(store: &Arc<Store>, id: Uuid) -> Step {
    let store = Arc::clone(store);
    Step::Run(
        Box::new(move || {
            store.remove_upload(id).unwrap();
        }),
        Box::new(Step::Json(200, committed())),
    )
}

#[tokio::test]
async fn an_upload_removed_while_its_commit_runs_answers_not_found_and_frees_the_object() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Arc::new(Store::open_in_memory(&[5; 32]).unwrap());
    let id = queued(&store);
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.on(
        "POST",
        &format!("/chat/media/{IMAGE}/commit"),
        removed_during_commit(&store, id),
    );
    rig.server.json(
        "POST",
        "/chat/media/delete",
        json!({"deleted": 1, "freed": 176_128, "used": 0, "quota": QUOTA}),
    );

    queue::reserve(api, &store, id, START_MS).await.unwrap();
    uploaded(&store, id);
    assert_eq!(
        queue::commit(api, &store, id, START_MS).await.unwrap_err(),
        ChatError::NotFound
    );
    assert!(store.upload(id).unwrap().is_none());
    let deletes = rig.server.calls_to("POST", "/chat/media/delete");
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].body, json!({"ids": [IMAGE]}));
}

#[tokio::test]
async fn a_failed_cleanup_after_a_removed_upload_still_answers_not_found() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Arc::new(Store::open_in_memory(&[5; 32]).unwrap());
    let id = queued(&store);
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.on(
        "POST",
        &format!("/chat/media/{IMAGE}/commit"),
        removed_during_commit(&store, id),
    );
    rig.server
        .on("POST", "/chat/media/delete", Step::Fail(ChatError::Offline));

    queue::reserve(api, &store, id, START_MS).await.unwrap();
    uploaded(&store, id);
    assert_eq!(
        queue::commit(api, &store, id, START_MS).await.unwrap_err(),
        ChatError::NotFound
    );
    assert!(!rig.server.calls_to("POST", "/chat/media/delete").is_empty());
    assert!(store.upload(id).unwrap().is_none());
}

#[tokio::test]
async fn a_second_commit_of_a_committed_upload_stays_ok_and_deletes_nothing() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    let commit = format!("/chat/media/{IMAGE}/commit");
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.json("POST", &commit, committed());
    rig.server.json("POST", &commit, committed());

    queue::reserve(api, &store, id, START_MS).await.unwrap();
    uploaded(&store, id);
    queue::commit(api, &store, id, START_MS).await.unwrap();
    queue::commit(api, &store, id, START_MS + 1).await.unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Committed
    );
    assert!(rig.server.calls_to("POST", "/chat/media/delete").is_empty());
}

#[test]
fn committing_an_unknown_upload_is_not_found() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    assert_eq!(
        store
            .upload_committed(Uuid::new_v4(), START_MS)
            .unwrap_err(),
        ChatError::NotFound
    );
}
