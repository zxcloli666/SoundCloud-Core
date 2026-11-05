use std::path::PathBuf;

use sc_chat::ChatError;
use sc_chat::store::{Store, UploadObject, UploadState};
use uuid::Uuid;

use crate::queue::{IMAGE_ID, MASTER, reserved, upload};

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
