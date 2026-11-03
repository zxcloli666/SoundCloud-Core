use std::path::PathBuf;

use sc_chat::media::prepare_image;
use sc_chat::store::{NewUpload, Reserved, Store, UploadObject, UploadState};
use sc_chat::{ChatError, MediaKind};
use uuid::Uuid;

use crate::support::*;

const MASTER: [u8; 32] = [5; 32];
const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const IMAGE_ID: Uuid = Uuid::from_u128(0x1a);
const THUMB_ID: Uuid = Uuid::from_u128(0x1b);

fn upload(album: Uuid, position: u32) -> NewUpload {
    let prepared = prepare_image(&jpeg(&photo(64, 48))).unwrap();
    NewUpload {
        album,
        position,
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        image_ref: format!("media/{album}/{position}.img"),
        thumb_ref: Some(format!("media/{album}/{position}.thumb")),
        attachment: prepared.attachment,
    }
}

fn reserved() -> Reserved {
    Reserved {
        media_id: IMAGE_ID,
        thumb_media_id: Some(THUMB_ID),
        presign_expires_at: 10_000,
    }
}

fn temp_db() -> PathBuf {
    std::env::temp_dir().join(format!("sc-chat-media-{}.db", Uuid::new_v4()))
}

fn remove(path: &std::path::Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(name));
    }
}

#[test]
fn an_album_queues_in_order_and_walks_every_state() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let album = Uuid::new_v4();
    let ids: Vec<Uuid> = (0..3)
        .map(|position| store.enqueue_upload(&upload(album, position), 100).unwrap())
        .collect();
    assert_eq!(store.album_uploads(album).unwrap().len(), 3);

    let first = store.next_upload().unwrap().unwrap();
    assert_eq!((first.id, first.state), (ids[0], UploadState::Queued));

    store.upload_reserved(ids[0], &reserved(), 200).unwrap();
    let after = store.upload(ids[0]).unwrap().unwrap();
    assert_eq!(after.state, UploadState::Reserved);
    assert_eq!(after.media_id, Some(IMAGE_ID));
    assert_eq!(after.conversation_id, CONVERSATION);
    assert_eq!(after.attachment.media_id, IMAGE_ID.to_string());
    assert_eq!(
        after.attachment.thumb.as_ref().unwrap().media_id,
        THUMB_ID.to_string()
    );
    assert!(!after.presign_expired(9_999) && after.presign_expired(10_000));

    store
        .upload_progress(ids[0], UploadObject::Image, 7, Some("stage-9"), 300)
        .unwrap();
    let mid = store.upload(ids[0]).unwrap().unwrap();
    assert_eq!(
        (
            mid.state,
            mid.object,
            mid.parts_sent,
            mid.stage_id.as_deref()
        ),
        (
            UploadState::Uploading,
            UploadObject::Image,
            7,
            Some("stage-9")
        )
    );

    store
        .upload_object_done(ids[0], UploadObject::Image, 400)
        .unwrap();
    let thumb_turn = store.upload(ids[0]).unwrap().unwrap();
    assert_eq!(
        (
            thumb_turn.object,
            thumb_turn.parts_sent,
            thumb_turn.stage_id
        ),
        (UploadObject::Thumb, 0, None)
    );
    store
        .upload_object_done(ids[0], UploadObject::Thumb, 500)
        .unwrap();
    assert_eq!(
        store.upload(ids[0]).unwrap().unwrap().state,
        UploadState::Uploaded
    );
    store.upload_committed(ids[0], 600).unwrap();
    assert_eq!(
        store.upload(ids[0]).unwrap().unwrap().state,
        UploadState::Committed
    );
    assert_eq!(store.next_upload().unwrap().unwrap().id, ids[1]);
}

#[test]
fn a_commit_needs_an_uploaded_object() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let id = store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    store.upload_committed(id, 2).unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Queued
    );
}

#[test]
fn a_fresh_presign_for_the_same_ids_keeps_the_progress() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let id = store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    store.upload_reserved(id, &reserved(), 2).unwrap();
    store
        .upload_object_done(id, UploadObject::Image, 3)
        .unwrap();
    store
        .upload_progress(id, UploadObject::Thumb, 2, Some("stage-1"), 4)
        .unwrap();

    let fresh = Reserved {
        presign_expires_at: 90_000,
        ..reserved()
    };
    store.upload_reserved(id, &fresh, 5).unwrap();
    let kept = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (
            kept.state,
            kept.object,
            kept.parts_sent,
            kept.stage_id.as_deref()
        ),
        (
            UploadState::Uploading,
            UploadObject::Thumb,
            2,
            Some("stage-1")
        )
    );
    assert_eq!(kept.presign_expires_at, Some(90_000));

    let other = Reserved {
        media_id: Uuid::from_u128(0x2a),
        thumb_media_id: Some(Uuid::from_u128(0x2b)),
        presign_expires_at: 95_000,
    };
    store.upload_reserved(id, &other, 6).unwrap();
    let reset = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (reset.state, reset.object, reset.parts_sent, reset.stage_id),
        (UploadState::Reserved, UploadObject::Image, 0, None)
    );
    assert_eq!(reset.media_id, Some(other.media_id));
    assert_eq!(
        reset.attachment.thumb.unwrap().media_id,
        other.thumb_media_id.unwrap().to_string()
    );
}

#[test]
fn a_missing_object_goes_back_to_its_put() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let id = store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    store.upload_reserved(id, &reserved(), 2).unwrap();
    store
        .upload_object_done(id, UploadObject::Image, 3)
        .unwrap();
    store
        .upload_object_done(id, UploadObject::Thumb, 4)
        .unwrap();
    store.upload_redo(id, UploadObject::Thumb, 5).unwrap();
    let redo = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (redo.state, redo.object, redo.parts_sent),
        (UploadState::Uploading, UploadObject::Thumb, 0)
    );
    store.upload_committed(id, 6).unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Uploading
    );
}

#[test]
fn failures_count_and_a_hard_one_ends_the_upload() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let id = store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    store.upload_failed(id, true, 2).unwrap();
    let soft = store.upload(id).unwrap().unwrap();
    assert_eq!((soft.state, soft.attempts), (UploadState::Queued, 1));
    store.upload_failed(id, false, 3).unwrap();
    let hard = store.upload(id).unwrap().unwrap();
    assert_eq!((hard.state, hard.attempts), (UploadState::Failed, 2));
    assert!(store.next_upload().unwrap().is_none());
}

#[test]
fn a_restart_clears_the_reserve_and_keeps_the_file() {
    let store = Store::open_in_memory(&MASTER).unwrap();
    let id = store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    store.upload_reserved(id, &reserved(), 2).unwrap();
    store
        .upload_progress(id, UploadObject::Thumb, 3, Some("s"), 3)
        .unwrap();
    store.upload_restart(id, 4).unwrap();
    let after = store.upload(id).unwrap().unwrap();
    assert_eq!(after.state, UploadState::Queued);
    assert_eq!(
        (&after.media_id, &after.stage_id, after.parts_sent),
        (&None, &None, 0)
    );
    assert!(after.presign_expired(0));
    assert!(after.image_ref.ends_with(".img"));
}

#[test]
fn an_album_survives_an_app_restart_with_its_keys_sealed() {
    let path = temp_db();
    let album = Uuid::new_v4();
    let new = upload(album, 0);
    let key = new.attachment.key.clone();
    let id = {
        let store = Store::open(&path, &MASTER).unwrap();
        let id = store.enqueue_upload(&new, 10).unwrap();
        store.upload_reserved(id, &reserved(), 20).unwrap();
        store
            .upload_progress(id, UploadObject::Image, 12, Some("stage-3"), 30)
            .unwrap();
        id
    };

    let raw: Vec<u8> = ["", "-wal"]
        .iter()
        .flat_map(|suffix| {
            let mut name = path.as_os_str().to_owned();
            name.push(suffix);
            std::fs::read(PathBuf::from(name)).unwrap_or_default()
        })
        .collect();
    assert!(!raw.windows(key.len()).any(|part| part == key.as_slice()));

    let store = Store::open(&path, &MASTER).unwrap();
    let resumed = store.next_upload().unwrap().unwrap();
    assert_eq!(resumed.id, id);
    assert_eq!(
        (resumed.state, resumed.object, resumed.parts_sent),
        (UploadState::Uploading, UploadObject::Image, 12)
    );
    assert_eq!(resumed.stage_id.as_deref(), Some("stage-3"));
    assert_eq!(resumed.attachment.key, key);
    assert_eq!(resumed.attachment.media_id, IMAGE_ID.to_string());
    assert_eq!(resumed.media_id, Some(IMAGE_ID));

    let refs = store.remove_album_uploads(album).unwrap();
    assert_eq!(refs.len(), 1);
    assert!(store.album_uploads(album).unwrap().is_empty());
    drop(store);
    remove(&path);
}

#[test]
fn a_wrong_store_key_does_not_read_the_queue() {
    let path = temp_db();
    {
        let store = Store::open(&path, &MASTER).unwrap();
        store.enqueue_upload(&upload(Uuid::new_v4(), 0), 1).unwrap();
    }
    let reopened = Store::open(&path, &[6; 32]);
    assert_eq!(reopened.err(), Some(ChatError::Crypto));
    remove(&path);
}
