mod sync_support;

use sc_chat::api::wire::{
    MediaKind, MediaSelection, MediaUrl, MyMediaStatus, ReserveAnswer, ReserveMedia,
};
use sc_chat::media::queue::{self, PRESIGN_MARGIN_MS};
use sc_chat::sc_chat_wire::proto::{Attachment, Thumb};
use sc_chat::store::{NewUpload, Store, UploadObject, UploadState};
use sc_chat::{ChatError, MineOrder, MineQuery};
use serde_json::{Value, json};
use sync_support::{START_MS, Step, rig};
use uuid::Uuid;

const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const IMAGE: Uuid = Uuid::from_u128(0x1a);
const THUMB: Uuid = Uuid::from_u128(0x1b);
const QUOTA: u64 = 1_073_741_824;

fn put(bytes: u64) -> Value {
    json!({
        "url": "https://s3.scnative.dev/opaque?X-Amz-Signature=1",
        "headers": {"content-length": bytes.to_string(), "content-type": "application/octet-stream"},
    })
}

fn reserve_answer(image: Uuid, thumb: Uuid) -> Value {
    json!({
        "id": image,
        "put": put(151_552),
        "thumb": {"id": thumb, "put": put(24_576)},
        "expires_in_s": 600,
        "used": 176_128,
        "quota": QUOTA,
    })
}

fn error(status: u16, body: Value) -> Step {
    Step::Json(status, body)
}

fn queued(store: &Store) -> Uuid {
    let attachment = Attachment {
        size: 151_552,
        thumb: Some(Thumb {
            size: 24_576,
            ..Thumb::default()
        }),
        ..Attachment::default()
    };
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        image_ref: "media/a/0.img".into(),
        thumb_ref: "media/a/0.thumb".into(),
        attachment,
    };
    store.enqueue_upload(&upload, START_MS).unwrap()
}

#[tokio::test]
async fn every_media_route_keeps_its_contract_shape() {
    let rig = rig();
    let api = rig.engine.api();
    let commit = format!("/chat/media/{IMAGE}/commit");
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.json(
        "POST",
        &commit,
        json!({"id": IMAGE, "bytes": 176_128, "used": 176_128, "quota": QUOTA}),
    );
    rig.server.json(
        "POST",
        "/chat/media/urls",
        json!({"expires_in_s": 900, "items": [
            {"state": "ok", "id": IMAGE, "url": "https://s3.scnative.dev/a"},
            {"state": "deleted", "id": THUMB},
            {"state": "lost", "id": CONVERSATION},
        ]}),
    );
    rig.server.json(
        "GET",
        "/chat/media/usage",
        json!({"bytes": 176_128, "reserved": 0, "quota": QUOTA, "files": 1, "conversations": [
            {"conversation_id": CONVERSATION, "bytes": 176_128, "files": 1, "gone": false},
        ]}),
    );
    rig.server.json(
        "GET",
        "/chat/media/mine",
        json!({"items": [{
            "id": IMAGE, "kind": "image", "conversation_id": CONVERSATION, "thumb_id": THUMB,
            "bytes": 176_128, "status": "attached", "chats": 2, "created_at": START_MS,
        }], "next_cursor": "c2"}),
    );
    rig.server.json(
        "POST",
        "/chat/media/delete",
        json!({"deleted": 1, "freed": 176_128, "used": 0, "quota": QUOTA}),
    );
    rig.server.json(
        "POST",
        "/chat/media/restore",
        json!({"restored": 1, "missed": 0, "used": 176_128, "quota": QUOTA}),
    );

    let reserve = ReserveMedia {
        upload_id: Uuid::from_u128(0x55),
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        bytes: 151_552,
        thumb_bytes: Some(24_576),
    };
    let reserved = api.reserve_media(&reserve).await.unwrap();
    assert_eq!(reserved.thumb.as_ref().unwrap().id, THUMB);
    assert_eq!(reserved.put.headers["content-length"], "151552");
    assert_eq!(api.commit_media(IMAGE).await.unwrap().bytes, 176_128);
    let urls = api
        .media_urls(&sc_chat::api::wire::UrlsRequest {
            conversation_id: Some(CONVERSATION),
            ids: vec![IMAGE, THUMB, CONVERSATION],
        })
        .await
        .unwrap();
    assert_eq!(
        urls.items,
        vec![
            MediaUrl::Ok {
                id: IMAGE,
                url: "https://s3.scnative.dev/a".into()
            },
            MediaUrl::Deleted { id: THUMB },
            MediaUrl::Lost { id: CONVERSATION },
        ]
    );
    assert_eq!(api.media_usage().await.unwrap().conversations.len(), 1);
    let mine = api
        .media_mine(&MineQuery {
            order: MineOrder::Big,
            conversation_id: Some(CONVERSATION),
            cursor: Some("c1".into()),
            limit: 50,
        })
        .await
        .unwrap();
    assert_eq!(mine.items[0].status, MyMediaStatus::Attached);
    assert_eq!(mine.next_cursor.as_deref(), Some("c2"));
    let deleted = api
        .delete_media(&MediaSelection::ids(vec![IMAGE]))
        .await
        .unwrap();
    assert_eq!(deleted.freed, 176_128);
    assert_eq!(
        api.restore_media(&MediaSelection::all())
            .await
            .unwrap()
            .restored,
        1
    );

    let calls = rig.server.calls();
    let body = |path: &str| {
        calls
            .iter()
            .find(|call| call.path == path)
            .map(|call| call.body.clone())
            .unwrap()
    };
    assert_eq!(
        body("/chat/media/reserve"),
        json!({
            "upload_id": Uuid::from_u128(0x55), "conversation_id": CONVERSATION,
            "kind": "image", "bytes": 151_552, "thumb_bytes": 24_576,
        })
    );
    assert_eq!(body(&commit), Value::Null);
    assert_eq!(
        body("/chat/media/urls"),
        json!({"conversation_id": CONVERSATION, "ids": [IMAGE, THUMB, CONVERSATION]})
    );
    assert_eq!(body("/chat/media/delete"), json!({"ids": [IMAGE]}));
    assert_eq!(body("/chat/media/restore"), json!({"all": true}));
    let mine_call = calls
        .iter()
        .find(|call| call.path == "/chat/media/mine")
        .unwrap();
    assert_eq!(
        mine_call.query,
        format!("order=big&conversation_id={CONVERSATION}&cursor=c1&limit=50")
    );
}

#[tokio::test]
async fn media_refusals_arrive_as_their_own_errors() {
    let rig = rig();
    let api = rig.engine.api();
    let reserve = ReserveMedia {
        upload_id: Uuid::new_v4(),
        conversation_id: CONVERSATION,
        kind: MediaKind::Avatar,
        bytes: 16_384,
        thumb_bytes: None,
    };
    for step in [
        error(
            403,
            json!({"error": "chat_quota_exceeded", "used": 1_000, "quota": QUOTA}),
        ),
        error(403, json!({"error": "chat_media_star_required"})),
        error(403, json!({"error": "chat_room_star_required"})),
        error(503, json!({"error": "chat_media_uploads_paused"})),
    ] {
        rig.server.on("POST", "/chat/media/reserve", step);
    }
    rig.server.on(
        "POST",
        &format!("/chat/media/{IMAGE}/commit"),
        error(
            409,
            json!({"error": "chat_media_missing", "ids": [THUMB, "not-a-uuid"]}),
        ),
    );

    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(api.reserve_media(&reserve).await.unwrap_err());
    }
    assert_eq!(
        seen,
        vec![
            ChatError::QuotaExceeded {
                used: 1_000,
                quota: QUOTA
            },
            ChatError::MediaStarRequired,
            ChatError::RoomStarRequired,
            ChatError::MediaUploadsPaused,
        ]
    );
    assert_eq!(
        api.commit_media(IMAGE).await.unwrap_err(),
        ChatError::MediaMissing { ids: vec![THUMB] }
    );
    assert_eq!(
        rig.server.calls_to("POST", "/chat/media/reserve")[0].body,
        json!({"upload_id": reserve.upload_id, "conversation_id": CONVERSATION, "kind": "avatar", "bytes": 16_384})
    );
}

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
    assert_eq!(targets.thumb.headers["content-length"], "24576");
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

#[test]
fn a_reserve_answer_for_an_avatar_has_no_thumb() {
    let answer: ReserveAnswer = serde_json::from_value(json!({
        "id": IMAGE, "put": {"url": "https://s3.scnative.dev/a"},
        "expires_in_s": 600, "used": 0, "quota": QUOTA,
    }))
    .unwrap();
    assert!(answer.thumb.is_none() && answer.put.headers.is_empty());
    assert!(
        serde_json::to_value(&answer)
            .unwrap()
            .get("thumb")
            .is_none()
    );
}
