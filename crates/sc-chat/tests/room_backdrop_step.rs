mod store_support;
mod sync_support;

use sc_chat::media::BackdropStep;
use sc_chat::store::{NewUpload, Reserved, Store, UploadObject};
use sc_chat::{BackdropChange, ChatError, ConversationKind, ConversationMeta, MediaKind};
use serde_json::json;
use store_support::{avatar, conv, store, summary};
use sync_support::{Step, rig};
use uuid::Uuid;

const MEDIA: Uuid = Uuid::from_u128(0x61);
const SETTINGS: &str = "/chat/rooms/ROOM42/settings";

fn room(store: &Store) -> Uuid {
    let id = conv();
    let mut seat = summary(id, ConversationKind::Room);
    seat.room_code = Some("ROOM42".into());
    store.put_conversations(&[seat]).unwrap();
    id
}

fn committed_backdrop(store: &Store, room: Uuid) -> Uuid {
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: room,
        kind: MediaKind::Backdrop,
        image_ref: "media/backdrop.img".into(),
        thumb_ref: None,
        attachment: avatar(0),
    };
    let upload_id = store.enqueue_upload(&upload, 1).unwrap();
    let reserved = Reserved {
        media_id: MEDIA,
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

fn shown(store: &Store, room: Uuid) {
    let mut image = avatar(0);
    image.media_id = MEDIA.to_string();
    let meta = ConversationMeta {
        avatar: Some(image),
        ..ConversationMeta::default()
    };
    store.put_room_meta(room, Some(&meta)).unwrap();
}

#[tokio::test]
async fn a_backdrop_goes_to_the_room_settings_and_hands_back_the_meta_with_its_key() {
    let rig = rig();
    let store = store();
    let id = room(&store);
    let upload_id = committed_backdrop(&store, id);
    rig.server.json("PUT", SETTINGS, json!({}));

    let step = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    let meta = step.send(rig.engine.api()).await.unwrap().unwrap();
    let image = meta.avatar.unwrap();
    assert_eq!(
        (image.media_id.as_str(), image.key.as_slice()),
        (MEDIA.to_string().as_str(), [7; 32].as_slice())
    );
    assert_eq!((meta.title.as_str(), meta.description.as_str()), ("", ""));
    let calls = rig.server.calls_to("PUT", SETTINGS);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].body["backdrop"],
        json!({"kind": "image", "mediaId": MEDIA})
    );
}

#[tokio::test]
async fn a_cleared_backdrop_sends_null_and_a_meta_without_an_image() {
    let rig = rig();
    let store = store();
    let id = room(&store);
    shown(&store, id);
    rig.server.json("PUT", SETTINGS, json!({}));

    let step = BackdropStep::set(&store, id, &BackdropChange::Clear).unwrap();
    assert_eq!(
        step.send(rig.engine.api()).await.unwrap(),
        Some(ConversationMeta::default())
    );
    let body = &rig.server.calls_to("PUT", SETTINGS)[0].body;
    assert_eq!(body.get("backdrop"), Some(&json!(null)));
}

#[tokio::test]
async fn a_preset_over_a_known_bare_meta_queues_nothing() {
    let rig = rig();
    let store = store();
    let id = room(&store);
    store
        .put_room_meta(id, Some(&ConversationMeta::default()))
        .unwrap();
    rig.server.json("PUT", SETTINGS, json!({}));

    let step = BackdropStep::preset(&store, id, "aurora".into()).unwrap();
    assert_eq!(step.send(rig.engine.api()).await.unwrap(), None);
    assert_eq!(
        rig.server.calls_to("PUT", SETTINGS)[0].body["backdrop"],
        json!({"kind": "preset", "id": "aurora"})
    );
}

#[tokio::test]
async fn a_preset_over_an_image_or_an_unknown_meta_hands_back_a_cleared_meta() {
    let rig = rig();
    let store = store();
    let with_image = room(&store);
    shown(&store, with_image);
    let unknown = room(&store);
    rig.server.json("PUT", SETTINGS, json!({}));
    rig.server.json("PUT", SETTINGS, json!({}));

    for id in [with_image, unknown] {
        let step = BackdropStep::preset(&store, id, "aurora".into()).unwrap();
        assert_eq!(
            step.send(rig.engine.api()).await.unwrap(),
            Some(ConversationMeta::default())
        );
    }
    assert_eq!(rig.server.calls_to("PUT", SETTINGS).len(), 2);
}

#[tokio::test]
async fn a_refused_settings_change_hands_back_no_meta() {
    let rig = rig();
    let store = store();
    let id = room(&store);
    let upload_id = committed_backdrop(&store, id);
    rig.server.on(
        "PUT",
        SETTINGS,
        Step::Json(403, json!({"error": "chat_room_star_required"})),
    );
    rig.server.on(
        "PUT",
        SETTINGS,
        Step::Json(
            403,
            json!({"error": "chat_backdrop_cooldown", "until": 1_800_000_600_000_i64}),
        ),
    );

    let set = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    assert_eq!(
        set.send(rig.engine.api()).await.unwrap_err(),
        ChatError::RoomStarRequired
    );
    let again = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    assert_eq!(
        again.send(rig.engine.api()).await.unwrap_err(),
        ChatError::BackdropCooldown {
            until: 1_800_000_600_000
        }
    );
}

#[test]
fn a_room_without_a_code_or_an_unknown_chat_never_reaches_the_network() {
    let store = store();
    let bare = conv();
    let odd = conv();
    store
        .put_conversations(&[
            summary(bare, ConversationKind::Room),
            summary(odd, ConversationKind::Unknown),
        ])
        .unwrap();

    for (id, expected) in [
        (bare, ChatError::NotFound),
        (odd, ChatError::Invalid),
        (conv(), ChatError::Invalid),
    ] {
        assert_eq!(
            BackdropStep::set(&store, id, &BackdropChange::Clear)
                .err()
                .unwrap_or(ChatError::Internal),
            expected
        );
        assert_eq!(
            BackdropStep::preset(&store, id, "aurora".into())
                .err()
                .unwrap_or(ChatError::Internal),
            expected
        );
    }
}
