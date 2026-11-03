mod sync_support;

use sc_chat::media::queue;
use sc_chat::sc_chat_wire::proto::{Attachment, Thumb};
use sc_chat::store::{NewUpload, Store, UploadObject, UploadState};
use sc_chat::{ChatError, MediaKind};
use serde_json::{Value, json};
use sync_support::{START_MS, Step, rig};
use uuid::Uuid;

const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const AVATAR: Uuid = Uuid::from_u128(0x3a);

fn queued(store: &Store) -> Uuid {
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        kind: MediaKind::Avatar,
        image_ref: "media/avatar.img".into(),
        thumb_ref: None,
        attachment: Attachment {
            size: 16_384,
            ..Attachment::default()
        },
    };
    store.enqueue_upload(&upload, START_MS).unwrap()
}

fn answer() -> Value {
    json!({
        "id": AVATAR,
        "put": {"url": "https://s3.scnative.dev/a", "headers": {"content-length": "16384"}},
        "expires_in_s": 600,
        "used": 16_384,
        "quota": 1_073_741_824,
    })
}

#[tokio::test]
async fn an_avatar_reserves_and_commits_without_a_thumb() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    rig.server.json("POST", "/chat/media/reserve", answer());
    rig.server.json(
        "POST",
        &format!("/chat/media/{AVATAR}/commit"),
        json!({"id": AVATAR, "bytes": 16_384, "used": 16_384, "quota": 1_073_741_824}),
    );

    let targets = queue::reserve(api, &store, id, START_MS).await.unwrap();
    assert!(targets.thumb.is_none());
    let sent = &rig.server.calls_to("POST", "/chat/media/reserve")[0].body;
    assert_eq!(
        sent,
        &json!({"upload_id": id, "conversation_id": CONVERSATION, "kind": "avatar", "bytes": 16_384})
    );
    store
        .upload_object_done(id, UploadObject::Image, START_MS + 1)
        .unwrap();
    queue::commit(api, &store, id, START_MS + 2).await.unwrap();
    let done = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (done.state, done.attachment.media_id),
        (UploadState::Committed, AVATAR.to_string())
    );
}

#[tokio::test]
async fn an_avatar_answer_with_a_thumb_is_a_protocol_error() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    let mut with_thumb = answer();
    with_thumb["thumb"] =
        json!({"id": Uuid::from_u128(0x3b), "put": {"url": "https://s3.scnative.dev/t"}});
    rig.server.json("POST", "/chat/media/reserve", with_thumb);

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
async fn a_missing_avatar_goes_back_to_its_one_put() {
    let rig = rig();
    let api = rig.engine.api();
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = queued(&store);
    rig.server.json("POST", "/chat/media/reserve", answer());
    rig.server.on(
        "POST",
        &format!("/chat/media/{AVATAR}/commit"),
        Step::Json(
            409,
            json!({"error": "chat_media_missing", "ids": [Uuid::from_u128(0x99)]}),
        ),
    );

    queue::reserve(api, &store, id, START_MS).await.unwrap();
    store
        .upload_object_done(id, UploadObject::Image, START_MS)
        .unwrap();
    assert!(matches!(
        queue::commit(api, &store, id, START_MS).await,
        Err(ChatError::MediaMissing { .. })
    ));
    let redo = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (redo.state, redo.object),
        (UploadState::Uploading, UploadObject::Image)
    );
}

#[test]
fn an_image_without_its_thumb_cannot_build_a_reserve() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        image_ref: "media/a.img".into(),
        thumb_ref: Some("media/a.thumb".into()),
        attachment: Attachment {
            size: 1_000,
            thumb: None::<Thumb>,
            ..Attachment::default()
        },
    };
    let id = store.enqueue_upload(&upload, START_MS).unwrap();
    let row = store.upload(id).unwrap().unwrap();
    assert_eq!(queue::reserve_request(&row).err(), Some(ChatError::Invalid));
}
