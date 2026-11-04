use std::sync::Arc;

use sc_chat::media::{AVATAR_SERVER_CAP, BLOCK, MediaWorker, open_image, prepare_avatar};
use sc_chat::store::{NewUpload, Reserved, Store, UploadObject, UploadState};
use sc_chat::{ChatError, MediaKind};
use uuid::Uuid;

use crate::support::*;

const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const AVATAR_ID: Uuid = Uuid::from_u128(0x3a);

fn avatar_upload() -> NewUpload {
    let prepared = prepare_avatar(&jpeg(&photo(64, 64))).unwrap();
    NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        kind: MediaKind::Avatar,
        image_ref: "media/avatar.img".into(),
        thumb_ref: None,
        attachment: prepared.attachment,
    }
}

#[test]
fn an_avatar_is_one_small_sealed_image_without_a_thumb() {
    let prepared = prepare_avatar(&jpeg(&photo(2000, 1500))).unwrap();
    let attachment = &prepared.attachment;
    assert!(attachment.thumb.is_none());
    assert_eq!((attachment.w, attachment.h), (512, 384));
    assert!(prepared.image.len() <= AVATAR_SERVER_CAP);
    assert!(prepared.image.len().is_multiple_of(BLOCK));
    assert_eq!(attachment.size, prepared.image.len() as u64);
    assert!(!attachment.thumbhash.is_empty());
    let worker = MediaWorker::new(1, std::time::Duration::from_secs(20));
    let opened = open_image(&worker, &prepared.image, attachment).unwrap();
    assert_eq!((opened.w, opened.h), (512, 384));
}

#[test]
fn the_queue_takes_a_thumb_exactly_for_an_image() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let mut with_thumb = avatar_upload();
    with_thumb.thumb_ref = Some("media/avatar.thumb".into());
    assert_eq!(
        store.enqueue_upload(&with_thumb, 1).err(),
        Some(ChatError::Invalid)
    );
    let mut image_without = avatar_upload();
    image_without.kind = MediaKind::Image;
    assert_eq!(
        store.enqueue_upload(&image_without, 1).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn an_avatar_is_uploaded_after_its_one_object() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let id = store.enqueue_upload(&avatar_upload(), 1).unwrap();
    let reserved = Reserved {
        media_id: AVATAR_ID,
        thumb_media_id: None,
        presign_expires_at: 10_000,
    };
    store.upload_reserved(id, &reserved, 2).unwrap();
    let after = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (after.kind, after.state, after.media_id),
        (MediaKind::Avatar, UploadState::Reserved, Some(AVATAR_ID))
    );
    assert_eq!(after.attachment.media_id, AVATAR_ID.to_string());
    assert!(after.attachment.thumb.is_none());

    store.upload_redo(id, UploadObject::Thumb, 3).unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().object,
        UploadObject::Image
    );
    store
        .upload_object_done(id, UploadObject::Thumb, 3)
        .unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Reserved
    );

    store
        .upload_object_done(id, UploadObject::Image, 4)
        .unwrap();
    let uploaded = store.upload(id).unwrap().unwrap();
    assert_eq!(
        (uploaded.state, uploaded.object),
        (UploadState::Uploaded, UploadObject::Image)
    );
    store.upload_committed(id, 5).unwrap();
    assert_eq!(
        store.upload(id).unwrap().unwrap().state,
        UploadState::Committed
    );
    assert_eq!(store.uploads().unwrap().len(), 1);
}

#[test]
fn every_queue_change_is_announced_while_watched() {
    let store = Store::open_in_memory(&[5; 32]).unwrap();
    let seen = Arc::new(Seen::default());
    store.enqueue_upload(&avatar_upload(), 1).unwrap();
    store.watch_uploads(Some(seen.clone()));

    let new = avatar_upload();
    let id = store.enqueue_upload(&new, 2).unwrap();
    store
        .upload_progress(id, UploadObject::Image, 0, Some("s"), 3)
        .unwrap();
    store
        .upload_reserved(
            id,
            &Reserved {
                media_id: AVATAR_ID,
                thumb_media_id: None,
                presign_expires_at: 10_000,
            },
            4,
        )
        .unwrap();
    store
        .upload_progress(id, UploadObject::Image, 3, Some("s"), 5)
        .unwrap();
    store.remove_album_uploads(new.album).unwrap();

    let events = seen.take();
    let views: Vec<_> = events
        .iter()
        .flat_map(|event| event.uploads.iter())
        .map(|view| (view.id, view.state, view.parts_sent, view.updated_at))
        .collect();
    assert_eq!(
        views,
        vec![
            (id, UploadState::Queued, 0, 2),
            (id, UploadState::Reserved, 0, 4),
            (id, UploadState::Uploading, 3, 5),
        ]
    );
    let first = &events[0].uploads[0];
    assert_eq!(
        (first.kind, first.image_bytes, first.thumb_bytes),
        (MediaKind::Avatar, new.attachment.size, 0)
    );
    assert_eq!(events.last().unwrap().removed, vec![id]);
    assert!(events.last().unwrap().uploads.is_empty());

    store.watch_uploads(None);
    store.enqueue_upload(&avatar_upload(), 6).unwrap();
    assert!(seen.take().is_empty());
}
