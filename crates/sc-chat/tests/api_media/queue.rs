use sc_chat::ChatError;
use sc_chat::api::wire::MediaUrl;
use sc_chat::media::queue::{self, PRESIGN_MARGIN_MS};
use sc_chat::store::{Store, UploadObject, UploadState};
use serde_json::{Value, json};
use uuid::Uuid;

use super::sync_support::{START_MS, rig};
use super::{IMAGE, QUOTA, THUMB, error, queued, reserve_answer};

#[tokio::test]
async fn the_queue_reserves_commits_and_keeps_its_ids_across_a_fresh_presign() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    let commit = format!("/chat/media/{IMAGE}/commit");
    for _ in 0..2 {
        rig.server
            .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    }
    rig.server.json(
        "POST",
        &commit,
        json!({"id": IMAGE, "bytes": 176_128, "used": 176_128, "quota": QUOTA}),
    );

    let targets = queue::reserve(api, &store, id, START_MS).await.unwrap();
    assert_eq!(targets.expires_at, START_MS + 600_000 - PRESIGN_MARGIN_MS);
    assert_eq!(targets.thumb.unwrap().headers["content-length"], "24576");
    let sent = &rig.server.calls_to("POST", "/chat/media/reserve")[0].body;
    assert_eq!(
        (&sent["upload_id"], &sent["bytes"], &sent["thumb_bytes"]),
        (&json!(id), &json!(151_552), &json!(24_576))
    );
    store
        .upload_object_done(id, UploadObject::Image, START_MS + 1)
        .unwrap();

    queue::reserve(api, &store, id, START_MS + 700_000)
        .await
        .unwrap();
    let resumed = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (resumed.state, resumed.object, resumed.media_id),
        (UploadState::Uploading, UploadObject::Thumb, Some(IMAGE))
    );
    assert!(!resumed.presign_expired(START_MS + 700_001));

    store
        .upload_object_done(id, UploadObject::Thumb, START_MS + 2)
        .unwrap();
    queue::commit(api, &store, id, START_MS + 3).await.unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Committed
    );
}

#[tokio::test]
async fn a_failed_commit_sends_the_queue_back_to_the_right_step() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    let commit = format!("/chat/media/{IMAGE}/commit");
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.on(
        "POST",
        &commit,
        error(409, json!({"error": "chat_media_missing", "ids": [IMAGE]})),
    );
    rig.server.on(
        "POST",
        &commit,
        error(404, json!({"error": "chat_not_found"})),
    );

    queue::reserve(api, &store, id, START_MS).await.unwrap();
    for object in [UploadObject::Image, UploadObject::Thumb] {
        store.upload_object_done(id, object, START_MS).unwrap();
    }
    assert_eq!(
        queue::commit(api, &store, id, START_MS).await.unwrap_err(),
        ChatError::MediaMissing { ids: vec![IMAGE] }
    );
    let redo = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (redo.state, redo.object),
        (UploadState::Uploading, UploadObject::Image)
    );

    for object in [UploadObject::Image, UploadObject::Thumb] {
        store.upload_object_done(id, object, START_MS).unwrap();
    }
    assert_eq!(
        queue::commit(api, &store, id, START_MS).await.unwrap_err(),
        ChatError::NotFound
    );
    let restarted = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (restarted.state, restarted.media_id),
        (UploadState::Queued, None)
    );
}

#[tokio::test]
async fn a_reserve_without_a_thumb_for_an_image_is_a_protocol_error() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    let mut answer = reserve_answer(IMAGE, THUMB);
    answer.as_object_mut().unwrap().remove("thumb");
    rig.server.json("POST", "/chat/media/reserve", answer);

    assert_eq!(
        queue::reserve(api, &store, id, START_MS).await.unwrap_err(),
        ChatError::Protocol
    );
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Queued
    );
}

#[tokio::test]
async fn urls_go_in_chunks_of_a_hundred_and_keep_the_order() {
    let rig = rig();
    let api = rig.engine.api();
    let ids: Vec<Uuid> = (1..=150).map(Uuid::from_u128).collect();
    for (chunk, ttl) in ids.chunks(100).zip([900, 600]) {
        let items: Vec<Value> = chunk
            .iter()
            .map(|id| json!({"state": "deleted", "id": id}))
            .collect();
        rig.server.json(
            "POST",
            "/chat/media/urls",
            json!({"expires_in_s": ttl, "items": items}),
        );
    }

    let answer = queue::urls(api, None, &ids).await.unwrap();
    assert_eq!(answer.expires_in_s, 600);
    let back: Vec<Uuid> = answer.items.iter().map(MediaUrl::id).collect();
    assert_eq!(back, ids);
    let calls = rig.server.calls_to("POST", "/chat/media/urls");
    assert_eq!(calls.len(), 2);
    assert!(calls[0].body.get("conversation_id").is_none());
    assert_eq!(calls[1].body["ids"].as_array().unwrap().len(), 50);

    let none = queue::urls(api, None, &[]).await.unwrap();
    assert!(none.items.is_empty());
}
