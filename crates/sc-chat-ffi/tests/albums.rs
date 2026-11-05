mod support;

use sc_chat::{AvatarChange, BackdropChange, ChatError, ConversationKind, RichText, SendInput};
use sc_chat_ffi::{NewMediaUpload, PreparedMedia};
use support::{Rig, committed, jpeg, settle, summary};
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

fn input(text: &str) -> SendInput {
    SendInput {
        conversation_id: ROOM,
        client_id: None,
        text: RichText {
            text: text.into(),
            spans: Vec::new(),
        },
        reply_to: None,
        embeds: Vec::new(),
        forward: None,
    }
}

#[tokio::test]
async fn an_album_is_sent_only_once_every_image_is_committed() {
    let rig = Rig::new();
    rig.with_store().await;
    let album = Uuid::new_v4();
    let prepared = settle(rig.client.prepare_image(jpeg(300, 200)))
        .await
        .unwrap();
    let row = settle(
        rig.client
            .enqueue_upload(upload(&prepared, album, Some("a.thumb"))),
    )
    .await
    .unwrap();

    let early = settle(rig.client.send_album(input("смотри"), album)).await;
    assert_eq!(early.err(), Some(ChatError::NotReady));
    let unknown = settle(rig.client.send_album(input("x"), Uuid::new_v4())).await;
    assert_eq!(unknown.err(), Some(ChatError::NotFound));

    committed(&rig.side_store(), row.id, 0x61, Some(0x62));

    let sent = settle(rig.client.send_album(input("смотри"), album))
        .await
        .unwrap();
    let ids: Vec<Uuid> = sent
        .attachments
        .iter()
        .map(|shown| shown.media_id)
        .collect();
    assert_eq!(ids, vec![Uuid::from_u128(0x61)]);
    assert_eq!((sent.attachments[0].w, sent.attachments[0].h), (300, 200));
}

#[tokio::test]
async fn avatar_and_backdrop_changes_take_only_committed_uploads() {
    let rig = Rig::new();
    rig.with_store().await;
    let group = Uuid::from_u128(0x6b);
    let mut seat = summary(ROOM, ConversationKind::Room);
    seat.room_code = Some("ROOM42".into());
    rig.side_store()
        .put_conversations(&[seat, summary(group, ConversationKind::Group)])
        .unwrap();
    let missing = settle(rig.client.set_group_avatar(
        group,
        AvatarChange::Set {
            upload_id: Uuid::new_v4(),
        },
    ))
    .await;
    assert_eq!(missing.err(), Some(ChatError::NotFound));

    let prepared = settle(rig.client.prepare_backdrop(jpeg(800, 450)))
        .await
        .unwrap();
    let row = settle(
        rig.client
            .enqueue_upload(upload(&prepared, Uuid::new_v4(), None)),
    )
    .await
    .unwrap();
    let pending = settle(
        rig.client
            .set_room_backdrop(ROOM, BackdropChange::Set { upload_id: row.id }),
    )
    .await;
    assert_eq!(pending.err(), Some(ChatError::Invalid));
    assert_eq!(rig.side_store().room_backdrop(ROOM).unwrap(), None);
}

#[tokio::test]
async fn a_room_album_takes_at_most_four_images() {
    let rig = Rig::new();
    rig.with_store().await;
    let store = rig.side_store();
    let mut seat = summary(ROOM, ConversationKind::Room);
    seat.room_code = Some("ROOM42".into());
    store.put_conversations(&[seat]).unwrap();
    let album = Uuid::new_v4();
    let prepared = settle(rig.client.prepare_image(jpeg(120, 80)))
        .await
        .unwrap();
    for position in 0..5u32 {
        let mut next = upload(&prepared, album, Some("a.thumb"));
        next.position = position;
        let row = settle(rig.client.enqueue_upload(next)).await.unwrap();
        committed(
            &store,
            row.id,
            0x70 + u128::from(position) * 2,
            Some(0x71 + u128::from(position) * 2),
        );
    }
    let five = settle(rig.client.send_album(input("пять"), album)).await;
    assert_eq!(five.err(), Some(ChatError::Invalid));

    let last = settle(rig.client.album_uploads(album))
        .await
        .unwrap()
        .pop()
        .unwrap();
    store
        .put_conversations(&[summary(ROOM, ConversationKind::Group)])
        .unwrap();
    let sent = settle(rig.client.send_album(input("пять"), album))
        .await
        .unwrap();
    assert_eq!(sent.attachments.len(), 5);
    assert_eq!(sent.attachments[4].media_id, last.media_id.unwrap());
}
