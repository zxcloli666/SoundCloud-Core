mod store_support;

use sc_chat::store::{NewUpload, Reserved, Store, UploadObject};
use sc_chat::{BackdropChange, ChatError, ConversationKind, ConversationMeta, MediaKind};
use store_support::{avatar, conv, count, store, summary};
use uuid::Uuid;

fn room(store: &Store) -> Uuid {
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Room)])
        .unwrap();
    id
}

fn upload(store: &Store, room: Uuid, kind: MediaKind, media: u128, commit: bool) -> Uuid {
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: room,
        kind,
        image_ref: "media/backdrop.img".into(),
        thumb_ref: None,
        attachment: avatar(0),
    };
    let upload_id = store.enqueue_upload(&upload, 1).unwrap();
    let reserved = Reserved {
        media_id: Uuid::from_u128(media),
        thumb_media_id: None,
        presign_expires_at: 100,
    };
    store.upload_reserved(upload_id, &reserved, 2).unwrap();
    store
        .upload_object_done(upload_id, UploadObject::Image, 3)
        .unwrap();
    if commit {
        store.upload_committed(upload_id, 4).unwrap();
    }
    upload_id
}

fn backdrop(n: u128) -> sc_chat::sc_chat_wire::proto::Attachment {
    let mut attachment = avatar(0);
    attachment.media_id = Uuid::from_u128(n).to_string();
    attachment
}

fn set(upload_id: Uuid) -> BackdropChange {
    BackdropChange::Set { upload_id }
}

#[test]
fn a_room_meta_carries_a_committed_backdrop_and_nothing_else() {
    let store = store();
    let id = room(&store);
    let upload_id = upload(&store, id, MediaKind::Backdrop, 0x61, true);

    let meta = store.backdrop_meta(id, &set(upload_id)).unwrap();
    assert_eq!(
        meta,
        ConversationMeta {
            title: String::new(),
            description: String::new(),
            avatar: Some(backdrop(0x61)),
        }
    );
    assert_eq!(
        store.backdrop_meta(id, &BackdropChange::Clear).unwrap(),
        ConversationMeta::default()
    );
}

#[test]
fn only_a_committed_backdrop_of_this_room_is_taken() {
    let store = store();
    let id = room(&store);
    let other = room(&store);
    let foreign = upload(&store, other, MediaKind::Backdrop, 0x61, true);
    let avatar_upload = upload(&store, id, MediaKind::Avatar, 0x62, true);
    let pending = upload(&store, id, MediaKind::Backdrop, 0x63, false);
    for upload_id in [foreign, avatar_upload, pending] {
        assert_eq!(
            store.backdrop_meta(id, &set(upload_id)).err(),
            Some(ChatError::Invalid)
        );
    }
    assert_eq!(
        store.backdrop_meta(id, &set(Uuid::new_v4())).err(),
        Some(ChatError::NotFound)
    );

    let group = conv();
    store
        .put_conversations(&[summary(group, ConversationKind::Group)])
        .unwrap();
    let in_group = upload(&store, group, MediaKind::Backdrop, 0x64, true);
    assert_eq!(
        store.backdrop_meta(group, &set(in_group)).err(),
        Some(ChatError::Invalid)
    );
    assert_eq!(
        store.backdrop_meta(group, &BackdropChange::Clear).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn the_room_backdrop_follows_the_latest_meta() {
    let store = store();
    let id = room(&store);
    assert_eq!(store.room_backdrop(id).unwrap(), None);

    let first = ConversationMeta {
        avatar: Some(backdrop(0x61)),
        ..ConversationMeta::default()
    };
    store.put_room_meta(id, Some(&first)).unwrap();
    assert_eq!(store.room_backdrop(id).unwrap(), Some(backdrop(0x61)));
    assert_eq!(
        store.attachment(id, Uuid::from_u128(0x61)).unwrap(),
        Some(backdrop(0x61))
    );

    let second = ConversationMeta {
        avatar: Some(backdrop(0x62)),
        ..ConversationMeta::default()
    };
    store.put_room_meta(id, Some(&second)).unwrap();
    assert_eq!(store.room_backdrop(id).unwrap(), Some(backdrop(0x62)));
    assert_eq!(store.attachment(id, Uuid::from_u128(0x61)).unwrap(), None);
    assert_eq!(
        store.attachment(id, Uuid::from_u128(0x62)).unwrap(),
        Some(backdrop(0x62))
    );

    store
        .put_room_meta(id, Some(&ConversationMeta::default()))
        .unwrap();
    assert_eq!(store.room_backdrop(id).unwrap(), None);
    assert_eq!(store.attachment(id, Uuid::from_u128(0x62)).unwrap(), None);

    store.put_room_meta(id, Some(&second)).unwrap();
    store.put_room_meta(id, None).unwrap();
    assert_eq!(store.room_backdrop(id).unwrap(), None);
    let rows = "SELECT count(*) FROM group_meta WHERE conversation_id = ?1";
    assert_eq!(count(&store, rows, id), 0);
}

#[test]
fn a_gone_room_takes_its_backdrop_key() {
    let store = store();
    let id = room(&store);
    let meta = ConversationMeta {
        avatar: Some(backdrop(0x61)),
        ..ConversationMeta::default()
    };
    store.put_room_meta(id, Some(&meta)).unwrap();
    store.purge_conversations(&[id]).unwrap();
    assert_eq!(store.room_backdrop(id).unwrap(), None);
    assert_eq!(store.attachment(id, Uuid::from_u128(0x61)).unwrap(), None);
}
