mod support;

use sc_chat::media::prepare_backdrop;
use sc_chat::store::Store;
use sc_chat::{BackdropChange, ChatError, ConversationKind, ConversationMeta, MediaKind};
use support::{Rig, committed, jpeg, settle, summary};
use uuid::Uuid;

const ROOM: Uuid = Uuid::from_u128(0x7e);
const MEDIA: Uuid = Uuid::from_u128(0x61);
const SETTINGS_REFUSED: ChatError = ChatError::Offline;

fn seeded(store: &Store) -> Vec<u8> {
    let prepared = prepare_backdrop(&jpeg(2400, 1350)).unwrap();
    let mut attachment = prepared.attachment;
    attachment.media_id = MEDIA.to_string();
    let mut seat = summary(ROOM, ConversationKind::Room);
    seat.room_code = Some("ROOM42".into());
    store.put_conversations(&[seat]).unwrap();
    let meta = ConversationMeta {
        avatar: Some(attachment),
        ..ConversationMeta::default()
    };
    store.put_room_meta(ROOM, Some(&meta)).unwrap();
    prepared.image
}

#[tokio::test]
async fn a_room_backdrop_resolves_and_opens_from_its_ciphertext() {
    let rig = Rig::new();
    rig.with_store().await;
    let ciphertext = seeded(&rig.side_store());

    let info = settle(rig.client.media_info(ROOM, MEDIA))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (info.media_id, info.w, info.h, info.size, info.thumb.clone()),
        (MEDIA, 1920, 1080, ciphertext.len() as u64, None)
    );
    assert!(!info.thumbhash.is_empty());

    let opened = settle(rig.client.open_image(ROOM, MEDIA, ciphertext.clone()))
        .await
        .unwrap();
    assert_eq!(
        (opened.w, opened.h, opened.mime.as_str()),
        (1920, 1080, "image/jpeg")
    );
    let decoded = image::load_from_memory(&opened.bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1920, 1080));

    let mut tampered = ciphertext.clone();
    tampered[100] ^= 1;
    assert_eq!(
        settle(rig.client.open_image(ROOM, MEDIA, tampered))
            .await
            .err(),
        Some(ChatError::Protocol)
    );
    assert_eq!(
        settle(rig.client.open_thumb(ROOM, MEDIA, ciphertext.clone()))
            .await
            .err(),
        Some(ChatError::NotFound)
    );
    let elsewhere = Uuid::from_u128(0x99);
    assert_eq!(
        settle(rig.client.open_image(elsewhere, MEDIA, ciphertext))
            .await
            .err(),
        Some(ChatError::NotFound)
    );
    assert_eq!(
        settle(rig.client.media_info(elsewhere, MEDIA))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn a_backdrop_change_goes_to_the_room_settings_before_any_meta_is_queued() {
    let rig = Rig::new();
    rig.with_store().await;
    let store = rig.side_store();
    seeded(&store);
    let prepared = settle(rig.client.prepare_backdrop(jpeg(800, 450)))
        .await
        .unwrap();
    let row = settle(rig.client.enqueue_upload(sc_chat_ffi::NewMediaUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: ROOM,
        image_ref: "media/b.img".into(),
        thumb_ref: None,
        seal: prepared.seal.clone(),
    }))
    .await
    .unwrap();
    assert_eq!(row.kind, MediaKind::Backdrop);
    committed(&store, row.id, 0x62, None);

    let refused = settle(
        rig.client
            .set_room_backdrop(ROOM, BackdropChange::Set { upload_id: row.id }),
    )
    .await;
    assert_eq!(refused.err(), Some(SETTINGS_REFUSED));
    let preset = settle(rig.client.set_room_preset(ROOM, "aurora".into())).await;
    assert_eq!(preset.err(), Some(SETTINGS_REFUSED));
    let thread = settle(rig.client.open_thread(ROOM)).await.unwrap();
    assert!(thread.messages.is_empty());
    assert_eq!(
        store
            .room_backdrop(ROOM)
            .unwrap()
            .map(|image| image.media_id),
        Some(MEDIA.to_string())
    );
}

#[tokio::test]
async fn a_backdrop_or_preset_aimed_at_a_group_is_refused_before_the_network() {
    let rig = Rig::new();
    rig.with_store().await;
    let group = Uuid::from_u128(0x6a);
    rig.side_store()
        .put_conversations(&[summary(group, ConversationKind::Group)])
        .unwrap();
    let preset = settle(rig.client.set_room_preset(group, "aurora".into())).await;
    assert_eq!(preset.err(), Some(ChatError::Invalid));
    let clear = settle(rig.client.set_room_backdrop(group, BackdropChange::Clear)).await;
    assert_eq!(clear.err(), Some(ChatError::Invalid));
}

#[tokio::test]
async fn a_room_without_a_known_code_is_not_found_before_the_network() {
    let rig = Rig::new();
    rig.with_store().await;
    rig.side_store()
        .put_conversations(&[summary(ROOM, ConversationKind::Room)])
        .unwrap();
    let preset = settle(rig.client.set_room_preset(ROOM, "aurora".into())).await;
    assert_eq!(preset.err(), Some(ChatError::NotFound));
    let clear = settle(rig.client.set_room_backdrop(ROOM, BackdropChange::Clear)).await;
    assert_eq!(clear.err(), Some(ChatError::NotFound));
}
