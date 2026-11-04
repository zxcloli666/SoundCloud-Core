mod support;

use sc_chat::{ChatError, MediaKind, UploadObject, UploadState};
use sc_chat_ffi::{NewMediaUpload, PreparedMedia};
use support::{Rig, jpeg, settle};
use uuid::Uuid;

const ROOM: Uuid = Uuid::from_u128(0x7e);

fn upload(prepared: &PreparedMedia, album: Uuid, thumb_ref: Option<&str>) -> NewMediaUpload {
    NewMediaUpload {
        album,
        position: 0,
        conversation_id: ROOM,
        image_ref: "media/a.img".into(),
        thumb_ref: thumb_ref.map(str::to_owned),
        seal: prepared.seal.clone(),
    }
}

#[tokio::test]
async fn every_kind_prepares_without_a_session() {
    let rig = Rig::new();
    let source = jpeg(2400, 1600);

    let backdrop = settle(rig.client.prepare_backdrop(source.clone()))
        .await
        .unwrap();
    assert_eq!(
        (
            backdrop.kind,
            backdrop.w,
            backdrop.h,
            backdrop.thumb.is_none()
        ),
        (MediaKind::Backdrop, 1920, 1280, true)
    );
    assert!(backdrop.image.len() <= 614_400 && backdrop.image.len() % 4096 == 0);
    assert!(!backdrop.thumbhash.is_empty());

    let image = settle(rig.client.prepare_image(source.clone()))
        .await
        .unwrap();
    assert_eq!(
        (image.kind, image.w, image.h),
        (MediaKind::Image, 1600, 1067)
    );
    assert!(
        image
            .thumb
            .as_ref()
            .is_some_and(|thumb| thumb.len() <= 32_768)
    );

    let avatar = settle(rig.client.prepare_avatar(source)).await.unwrap();
    assert_eq!(
        (avatar.kind, avatar.w, avatar.h),
        (MediaKind::Avatar, 512, 341)
    );
    assert!(avatar.image.len() <= 65_536 && avatar.thumb.is_none());

    let broken = settle(rig.client.prepare_backdrop(b"not an image".to_vec())).await;
    assert_eq!(broken.err(), Some(ChatError::Invalid));
    assert!(!rig.client.media_config().uploads);
}

#[tokio::test]
async fn the_queue_and_media_calls_wait_for_the_store() {
    let rig = Rig::new();
    let prepared = settle(rig.client.prepare_backdrop(jpeg(64, 48)))
        .await
        .unwrap();
    let queued = settle(
        rig.client
            .enqueue_upload(upload(&prepared, Uuid::new_v4(), None)),
    )
    .await;
    assert_eq!(queued.err(), Some(ChatError::NotReady));
    assert_eq!(
        settle(rig.client.room_backdrop(ROOM)).await.err(),
        Some(ChatError::NotReady)
    );
    assert_eq!(
        settle(rig.client.media_usage()).await.err(),
        Some(ChatError::NotReady)
    );
}

#[tokio::test]
async fn a_backdrop_upload_walks_the_queue_and_survives_a_failed_reserve() {
    let rig = Rig::new();
    rig.with_store().await;
    let prepared = settle(rig.client.prepare_backdrop(jpeg(2000, 1200)))
        .await
        .unwrap();
    let album = Uuid::new_v4();

    let with_thumb = settle(
        rig.client
            .enqueue_upload(upload(&prepared, album, Some("t"))),
    )
    .await;
    assert_eq!(with_thumb.err(), Some(ChatError::Invalid));

    let row = settle(rig.client.enqueue_upload(upload(&prepared, album, None)))
        .await
        .unwrap();
    assert_eq!(
        (row.kind, row.state, row.object, row.thumb_ref.clone()),
        (
            MediaKind::Backdrop,
            UploadState::Queued,
            UploadObject::Image,
            None
        )
    );
    assert_eq!(
        (row.image_bytes, row.thumb_bytes),
        (prepared.image.len() as u64, 0)
    );
    assert_eq!(
        settle(rig.client.upload(row.id)).await.unwrap(),
        Some(row.clone())
    );
    assert_eq!(
        settle(rig.client.uploads()).await.unwrap(),
        vec![row.clone()]
    );
    assert_eq!(
        settle(rig.client.album_uploads(album)).await.unwrap(),
        vec![row.clone()]
    );
    assert_eq!(
        settle(rig.client.next_upload()).await.unwrap(),
        Some(row.clone())
    );

    assert!(settle(rig.client.reserve_upload(row.id)).await.is_err());
    let kept = settle(rig.client.upload(row.id)).await.unwrap().unwrap();
    assert_eq!((kept.state, kept.media_id), (UploadState::Queued, None));

    settle(rig.client.upload_failed(row.id, true))
        .await
        .unwrap();
    settle(rig.client.upload_failed(row.id, false))
        .await
        .unwrap();
    let failed = settle(rig.client.upload(row.id)).await.unwrap().unwrap();
    assert_eq!((failed.state, failed.attempts), (UploadState::Failed, 2));
    settle(rig.client.upload_restart(row.id)).await.unwrap();
    let again = settle(rig.client.upload(row.id)).await.unwrap().unwrap();
    assert_eq!(again.state, UploadState::Queued);

    let removed = settle(rig.client.remove_album_uploads(album))
        .await
        .unwrap();
    assert_eq!(removed.len(), 1);
    assert!(settle(rig.client.uploads()).await.unwrap().is_empty());

    let events = rig.events.uploads();
    let states: Vec<UploadState> = events
        .iter()
        .flat_map(|event| event.uploads.iter().map(|view| view.state))
        .collect();
    assert_eq!(
        states,
        vec![
            UploadState::Queued,
            UploadState::Queued,
            UploadState::Failed,
            UploadState::Queued
        ]
    );
    assert_eq!(events.last().unwrap().removed, vec![row.id]);
}

#[tokio::test]
async fn one_upload_is_cancelled_and_its_album_keeps_the_rest() {
    let rig = Rig::new();
    rig.with_store().await;
    let prepared = settle(rig.client.prepare_backdrop(jpeg(64, 48)))
        .await
        .unwrap();
    let album = Uuid::new_v4();
    let first = settle(rig.client.enqueue_upload(upload(&prepared, album, None)))
        .await
        .unwrap();
    let second = settle(rig.client.enqueue_upload(NewMediaUpload {
        position: 1,
        image_ref: "media/b.img".into(),
        ..upload(&prepared, album, None)
    }))
    .await
    .unwrap();

    let removed = settle(rig.client.remove_upload(second.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (removed.id, removed.image_ref),
        (second.id, "media/b.img".into())
    );
    assert_eq!(
        settle(rig.client.album_uploads(album)).await.unwrap(),
        vec![first]
    );
    assert_eq!(
        settle(rig.client.remove_upload(second.id)).await.unwrap(),
        None
    );
    let events = rig.events.uploads();
    assert_eq!(events.last().unwrap().removed, vec![second.id]);
    assert_eq!(
        events
            .iter()
            .filter(|event| !event.removed.is_empty())
            .count(),
        1
    );
}
