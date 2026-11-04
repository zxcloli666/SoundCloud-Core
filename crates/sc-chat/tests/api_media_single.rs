mod sync_support;

use sc_chat::media::queue;
use sc_chat::sc_chat_wire::proto::{Attachment, Thumb};
use sc_chat::store::{MediaUpload, NewUpload, Store, UploadObject, UploadState};
use sc_chat::{ChatError, MediaKind};
use serde_json::{Value, json};
use sync_support::{START_MS, Step, rig};
use uuid::Uuid;

const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const MEDIA: Uuid = Uuid::from_u128(0x3a);
const SINGLE: [(MediaKind, &str, u64); 2] = [
    (MediaKind::Avatar, "avatar", 16_384),
    (MediaKind::Backdrop, "backdrop", 614_400),
];

fn queued(store: &Store, kind: MediaKind, bytes: u64) -> Uuid {
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        kind,
        image_ref: "media/single.img".into(),
        thumb_ref: None,
        attachment: Attachment {
            size: bytes,
            ..Attachment::default()
        },
    };
    store.enqueue_upload(&upload, START_MS).unwrap()
}

fn answer(bytes: u64) -> Value {
    json!({
        "id": MEDIA,
        "put": {"url": "https://s3.scnative.dev/a", "headers": {"content-length": bytes.to_string()}},
        "expires_in_s": 600,
        "used": bytes,
        "quota": 1_073_741_824,
    })
}

#[tokio::test]
async fn an_avatar_or_a_backdrop_reserves_and_commits_without_a_thumb() {
    for (kind, name, bytes) in SINGLE {
        let rig = rig();
        let api = rig.engine.api();
        let store = Store::open_in_memory(&[5; 32]).unwrap();
        let id = queued(&store, kind, bytes);
        rig.server
            .json("POST", "/chat/media/reserve", answer(bytes));
        rig.server.json(
            "POST",
            &format!("/chat/media/{MEDIA}/commit"),
            json!({"id": MEDIA, "bytes": bytes, "used": bytes, "quota": 1_073_741_824}),
        );

        let targets = queue::reserve(api, &store, id, START_MS).await.unwrap();
        assert!(targets.thumb.is_none());
        let sent = &rig.server.calls_to("POST", "/chat/media/reserve")[0].body;
        assert_eq!(
            sent,
            &json!({"upload_id": id, "conversation_id": CONVERSATION, "kind": name, "bytes": bytes}),
            "{name}"
        );
        store
            .upload_object_done(id, UploadObject::Thumb, START_MS + 1)
            .unwrap();
        assert_eq!(
            store.upload(id).unwrap().unwrap().state,
            UploadState::Reserved,
            "{name}"
        );
        store
            .upload_object_done(id, UploadObject::Image, START_MS + 1)
            .unwrap();
        assert_eq!(
            store.upload(id).unwrap().unwrap().state,
            UploadState::Uploaded,
            "{name}"
        );
        queue::commit(api, &store, id, START_MS + 2).await.unwrap();
        let done = store.upload(id).unwrap().unwrap();
        assert_eq!(
            (done.kind, done.state, done.attachment.media_id),
            (kind, UploadState::Committed, MEDIA.to_string())
        );
    }
}

#[tokio::test]
async fn a_single_object_answer_with_a_thumb_is_a_protocol_error() {
    for (kind, name, bytes) in SINGLE {
        let rig = rig();
        let api = rig.engine.api();
        let store = Store::open_in_memory(&[5; 32]).unwrap();
        let id = queued(&store, kind, bytes);
        let mut with_thumb = answer(bytes);
        with_thumb["thumb"] =
            json!({"id": Uuid::from_u128(0x3b), "put": {"url": "https://s3.scnative.dev/t"}});
        rig.server.json("POST", "/chat/media/reserve", with_thumb);

        assert_eq!(
            queue::reserve(api, &store, id, START_MS).await.unwrap_err(),
            ChatError::Protocol,
            "{name}"
        );
        assert_eq!(
            store.upload(id).unwrap().unwrap().state,
            UploadState::Queued
        );
    }
}

#[tokio::test]
async fn a_missing_single_object_goes_back_to_its_one_put() {
    for (kind, name, bytes) in SINGLE {
        let rig = rig();
        let api = rig.engine.api();
        let store = Store::open_in_memory(&[5; 32]).unwrap();
        let id = queued(&store, kind, bytes);
        rig.server
            .json("POST", "/chat/media/reserve", answer(bytes));
        rig.server.on(
            "POST",
            &format!("/chat/media/{MEDIA}/commit"),
            Step::Json(
                409,
                json!({"error": "chat_media_missing", "ids": [Uuid::from_u128(0x99)]}),
            ),
        );

        queue::reserve(api, &store, id, START_MS).await.unwrap();
        store
            .upload_object_done(id, UploadObject::Image, START_MS)
            .unwrap();
        assert!(
            matches!(
                queue::commit(api, &store, id, START_MS).await,
                Err(ChatError::MediaMissing { .. })
            ),
            "{name}"
        );
        let redo = store.upload(id).unwrap().unwrap();
        assert_eq!(
            (redo.state, redo.object),
            (UploadState::Uploading, UploadObject::Image)
        );
    }
}

#[test]
fn an_image_without_its_thumb_never_reaches_a_reserve() {
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
    assert_eq!(
        store.enqueue_upload(&upload, START_MS).err(),
        Some(ChatError::Invalid)
    );
    let row = MediaUpload {
        id: Uuid::new_v4(),
        album: upload.album,
        position: 0,
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        image_ref: upload.image_ref.clone(),
        thumb_ref: upload.thumb_ref.clone(),
        attachment: upload.attachment.clone(),
        state: UploadState::Queued,
        object: UploadObject::Image,
        media_id: None,
        stage_id: None,
        parts_sent: 0,
        presign_expires_at: None,
        attempts: 0,
        created_at: START_MS,
        updated_at: START_MS,
    };
    assert_eq!(queue::reserve_request(&row).err(), Some(ChatError::Invalid));
}

#[test]
fn a_single_object_kind_takes_no_thumb() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    for (kind, name, bytes) in SINGLE {
        let mut with_thumb_ref = NewUpload {
            album: Uuid::new_v4(),
            position: 0,
            conversation_id: CONVERSATION,
            kind,
            image_ref: "media/single.img".into(),
            thumb_ref: Some("media/single.thumb".into()),
            attachment: Attachment {
                size: bytes,
                ..Attachment::default()
            },
        };
        assert_eq!(
            store.enqueue_upload(&with_thumb_ref, START_MS).err(),
            Some(ChatError::Invalid),
            "{name}"
        );
        with_thumb_ref.thumb_ref = None;
        with_thumb_ref.attachment.thumb = Some(Thumb::default());
        assert_eq!(
            store.enqueue_upload(&with_thumb_ref, START_MS).err(),
            Some(ChatError::Invalid),
            "{name}"
        );
    }
}
