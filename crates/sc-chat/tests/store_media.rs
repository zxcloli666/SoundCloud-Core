mod store_support;

use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::store::{NewUpload, Reserved, Store, UploadObject};
use sc_chat::{
    AttachmentKind, AvatarChange, ChatError, ConversationKind, ConversationMeta, ConversationPatch,
    HistoryQuery, MediaKind, Message, MessageAttachment, MessageBody, MessageClass, MetaPatch,
    PreviewKind,
};
use store_support::{PEER, album, avatar, conv, count, envelope, image, open, row, store, summary};
use uuid::Uuid;

fn history(store: &Store, id: Uuid) -> Vec<Message> {
    let query = HistoryQuery {
        before: None,
        after: None,
        limit: 10,
    };
    store.messages(id, &query).unwrap().messages
}

fn shown(n: u128) -> MessageAttachment {
    MessageAttachment {
        media_id: Uuid::from_u128(n),
        kind: AttachmentKind::Image,
        size: 1_000,
        w: 640,
        h: 480,
        thumbhash: vec![1, 2, 3],
    }
}

fn group(store: &Store) -> Uuid {
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    id
}

fn committed_avatar(store: &Store, id: Uuid, n: u128) -> Uuid {
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: id,
        kind: MediaKind::Avatar,
        image_ref: "media/avatar.img".into(),
        thumb_ref: None,
        attachment: avatar(0),
    };
    let upload_id = store.enqueue_upload(&upload, 1).unwrap();
    let reserved = Reserved {
        media_id: Uuid::from_u128(n),
        thumb_media_id: None,
        presign_expires_at: 100,
    };
    store.upload_reserved(upload_id, &reserved, 2).unwrap();
    store
        .upload_object_done(upload_id, UploadObject::Image, 3)
        .unwrap();
    store.upload_committed(upload_id, 4).unwrap();
    upload_id
}

fn set(upload_id: Uuid) -> ConversationPatch {
    ConversationPatch {
        avatar: Some(AvatarChange::Set { upload_id }),
        ..ConversationPatch::default()
    }
}

#[test]
fn an_album_shows_without_keys_and_its_keys_stay_in_the_store() {
    let store = store();
    let id = group(&store);
    store.put_messages(&[album(id, 1, "", &[1, 2])]).unwrap();

    let message = &history(&store, id)[0];
    assert_eq!(message.attachments, vec![shown(1), shown(2)]);
    assert_eq!(
        store.attachment(id, Uuid::from_u128(2)).unwrap(),
        Some(image(2))
    );
    assert_eq!(store.attachment(conv(), Uuid::from_u128(2)).unwrap(), None);
    assert_eq!(store.attachment(id, Uuid::from_u128(3)).unwrap(), None);
    let preview = store.conversation(id).unwrap().unwrap().preview.unwrap();
    assert_eq!(preview.kind, PreviewKind::Image);
}

#[test]
fn a_deleted_album_drops_its_attachments_and_keys() {
    let store = store();
    let id = group(&store);
    let delete = row(
        id,
        2,
        PEER,
        MessageClass::Edit,
        Some(1),
        open(&envelope(Body::Delete(proto::Delete { target: 1 }))),
    );
    store
        .put_messages(&[album(id, 1, "смотри", &[1]), delete])
        .unwrap();

    let message = &history(&store, id)[0];
    assert_eq!(message.body, MessageBody::Deleted);
    assert!(message.attachments.is_empty());
    assert_eq!(store.attachment(id, Uuid::from_u128(1)).unwrap(), None);
}

#[test]
fn a_gone_conversation_takes_its_album_keys_and_avatar() {
    let store = store();
    let id = group(&store);
    store.put_messages(&[album(id, 1, "x", &[1])]).unwrap();
    let meta = ConversationMeta {
        title: "Group".into(),
        description: String::new(),
        avatar: Some(avatar(9)),
    };
    store.put_group_meta(id, &meta).unwrap();

    store.purge_conversations(&[id]).unwrap();

    assert_eq!(store.attachment(id, Uuid::from_u128(1)).unwrap(), None);
    assert_eq!(store.attachment(id, Uuid::from_u128(9)).unwrap(), None);
    for table in ["message_media", "group_meta"] {
        let sql = format!("SELECT count(*) FROM {table} WHERE conversation_id = ?1");
        assert_eq!(count(&store, &sql, id), 0);
    }
}

#[test]
fn the_group_avatar_resolves_only_while_it_is_current() {
    let store = store();
    let id = group(&store);
    let mut meta = ConversationMeta {
        title: "Group".into(),
        description: "d".into(),
        avatar: Some(avatar(9)),
    };
    store.put_group_meta(id, &meta).unwrap();
    assert_eq!(store.group_meta(id).unwrap(), Some(meta.clone()));
    assert_eq!(
        store.attachment(id, Uuid::from_u128(9)).unwrap(),
        Some(avatar(9))
    );

    meta.avatar = None;
    store.put_group_meta(id, &meta).unwrap();
    assert_eq!(store.attachment(id, Uuid::from_u128(9)).unwrap(), None);
    assert_eq!(store.group_meta(id).unwrap(), Some(meta));
}

#[test]
fn next_meta_keeps_sets_and_clears_the_avatar() {
    let store = store();
    let id = group(&store);
    let upload_id = committed_avatar(&store, id, 9);

    let rename = ConversationPatch {
        meta: Some(MetaPatch {
            title: Some("Ночная смена".into()),
            description: None,
        }),
        ..ConversationPatch::default()
    };
    let renamed = store.next_meta(id, &rename).unwrap();
    assert_eq!(
        renamed,
        ConversationMeta {
            title: "Ночная смена".into(),
            description: String::new(),
            avatar: None,
        }
    );

    let with_avatar = store.next_meta(id, &set(upload_id)).unwrap();
    let mut expected = avatar(0);
    expected.media_id = Uuid::from_u128(9).to_string();
    assert_eq!(with_avatar.avatar, Some(expected.clone()));
    assert_eq!(with_avatar.title, "Group");

    store.put_group_meta(id, &with_avatar).unwrap();
    let kept = store.next_meta(id, &rename).unwrap();
    assert_eq!(
        (kept.title.as_str(), kept.avatar),
        ("Ночная смена", Some(expected))
    );

    let clear = ConversationPatch {
        avatar: Some(AvatarChange::Clear),
        ..ConversationPatch::default()
    };
    assert_eq!(store.next_meta(id, &clear).unwrap().avatar, None);
}

#[test]
fn next_meta_takes_only_a_committed_avatar_of_this_group() {
    let store = store();
    let id = group(&store);
    let other = group(&store);
    let foreign = committed_avatar(&store, other, 9);
    assert_eq!(
        store.next_meta(id, &set(foreign)).err(),
        Some(ChatError::Invalid)
    );
    assert_eq!(
        store.next_meta(id, &set(Uuid::new_v4())).err(),
        Some(ChatError::NotFound)
    );

    let pending = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: id,
        kind: MediaKind::Avatar,
        image_ref: "media/avatar.img".into(),
        thumb_ref: None,
        attachment: avatar(0),
    };
    let pending = store.enqueue_upload(&pending, 1).unwrap();
    assert_eq!(
        store.next_meta(id, &set(pending)).err(),
        Some(ChatError::Invalid)
    );
}

#[test]
fn a_patch_without_avatar_reads_from_old_json() {
    let patch: ConversationPatch =
        serde_json::from_value(serde_json::json!({"meta": {"title": "t"}})).unwrap();
    assert_eq!(patch.avatar, None);
    let set: ConversationPatch = serde_json::from_value(serde_json::json!({
        "avatar": {"kind": "set", "uploadId": Uuid::from_u128(5)}
    }))
    .unwrap();
    assert_eq!(
        set.avatar,
        Some(AvatarChange::Set {
            upload_id: Uuid::from_u128(5)
        })
    );
    let clear: ConversationPatch =
        serde_json::from_value(serde_json::json!({"avatar": {"kind": "clear"}})).unwrap();
    assert_eq!(clear.avatar, Some(AvatarChange::Clear));
}
