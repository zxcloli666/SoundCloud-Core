use sc_chat::api::wire::RoomPreviewView;
use sc_chat::{BackdropChange, MediaKind, RoomBackdrop, RoomPreview, RoomSettingsPatch};
use serde_json::{Value, json};
use uuid::Uuid;

const MEDIA: Uuid = Uuid::from_u128(0x5e);

fn preview(backdrop: Option<Value>) -> RoomPreview {
    let mut view = json!({
        "code": "ABC234",
        "title": null,
        "host": {"id": "1", "name": "Host", "avatar_url": null},
        "track": null,
        "listeners": 2,
        "capacity": 25,
        "public": true,
        "alive": true,
        "star_host": true,
    });
    if let Some(backdrop) = backdrop {
        view["backdrop"] = backdrop;
    }
    let view: RoomPreviewView = serde_json::from_value(view).unwrap();
    RoomPreview::from(&view)
}

#[test]
fn a_room_backdrop_is_a_preset_or_an_image() {
    let preset = RoomBackdrop::Preset {
        id: "aurora".into(),
    };
    let image = RoomBackdrop::Image { media_id: MEDIA };
    assert_eq!(
        serde_json::to_value(&preset).unwrap(),
        json!({"kind": "preset", "id": "aurora"})
    );
    assert_eq!(
        serde_json::to_value(&image).unwrap(),
        json!({"kind": "image", "mediaId": MEDIA})
    );
    let snake: RoomBackdrop =
        serde_json::from_value(json!({"kind": "image", "media_id": MEDIA})).unwrap();
    assert_eq!(snake, image);
    assert!(serde_json::from_value::<RoomBackdrop>(json!({"kind": "video"})).is_err());
}

#[test]
fn room_settings_tell_a_cleared_backdrop_from_an_untouched_one() {
    let untouched = RoomSettingsPatch {
        title: Some("Night".into()),
        ..RoomSettingsPatch::default()
    };
    assert!(
        serde_json::to_value(&untouched)
            .unwrap()
            .get("backdrop")
            .is_none()
    );

    let cleared = RoomSettingsPatch {
        backdrop: Some(None),
        ..RoomSettingsPatch::default()
    };
    let sent = serde_json::to_value(&cleared).unwrap();
    assert_eq!(sent["backdrop"], Value::Null);
    assert_eq!(
        serde_json::from_value::<RoomSettingsPatch>(sent).unwrap(),
        cleared
    );

    let set = RoomSettingsPatch {
        backdrop: Some(Some(RoomBackdrop::Image { media_id: MEDIA })),
        ..RoomSettingsPatch::default()
    };
    let sent = serde_json::to_value(&set).unwrap();
    assert_eq!(sent["backdrop"], json!({"kind": "image", "mediaId": MEDIA}));
    assert_eq!(
        serde_json::from_value::<RoomSettingsPatch>(sent).unwrap(),
        set
    );

    let from_webview: RoomSettingsPatch =
        serde_json::from_value(json!({"backdrop": {"kind": "preset", "id": "dunes"}})).unwrap();
    assert_eq!(
        from_webview.backdrop,
        Some(Some(RoomBackdrop::Preset { id: "dunes".into() }))
    );
}

#[test]
fn a_backdrop_change_reads_like_an_avatar_change() {
    let set: BackdropChange =
        serde_json::from_value(json!({"kind": "set", "uploadId": MEDIA})).unwrap();
    assert_eq!(set, BackdropChange::Set { upload_id: MEDIA });
    let clear: BackdropChange = serde_json::from_value(json!({"kind": "clear"})).unwrap();
    assert_eq!(clear, BackdropChange::Clear);
    assert_eq!(
        serde_json::to_value(set).unwrap(),
        json!({"kind": "set", "uploadId": MEDIA})
    );
    assert_eq!(
        serde_json::to_value(MediaKind::Backdrop).unwrap(),
        json!("backdrop")
    );
}

#[test]
fn an_invite_preview_shows_a_preset_and_never_an_image() {
    assert_eq!(
        preview(Some(json!({"kind": "preset", "id": "aurora"}))).backdrop,
        Some(RoomBackdrop::Preset {
            id: "aurora".into()
        })
    );
    assert_eq!(
        preview(Some(json!({"kind": "image", "mediaId": MEDIA}))).backdrop,
        None
    );
    assert_eq!(
        preview(Some(json!({"kind": "video", "id": 3}))).backdrop,
        None
    );
    assert_eq!(preview(Some(Value::Null)).backdrop, None);
    assert_eq!(preview(None).backdrop, None);
    let shown =
        serde_json::to_value(preview(Some(json!({"kind": "preset", "id": "satin"})))).unwrap();
    assert_eq!(shown["backdrop"], json!({"kind": "preset", "id": "satin"}));
}
