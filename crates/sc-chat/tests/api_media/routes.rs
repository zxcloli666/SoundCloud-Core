use sc_chat::api::wire::{
    MediaKind, MediaSelection, MediaUrl, MyMediaStatus, ReserveAnswer, ReserveMedia,
};
use sc_chat::{ChatError, MineOrder, MineQuery};
use serde_json::{Value, json};
use uuid::Uuid;

use super::sync_support::{START_MS, rig};
use super::{CONVERSATION, IMAGE, QUOTA, THUMB, error, reserve_answer};

#[tokio::test]
async fn every_media_route_keeps_its_contract_shape() {
    let rig = rig();
    let api = rig.engine.api();
    let commit = format!("/chat/media/{IMAGE}/commit");
    rig.server
        .json("POST", "/chat/media/reserve", reserve_answer(IMAGE, THUMB));
    rig.server.json(
        "POST",
        &commit,
        json!({"id": IMAGE, "bytes": 176_128, "used": 176_128, "quota": QUOTA}),
    );
    rig.server.json(
        "POST",
        "/chat/media/urls",
        json!({"expires_in_s": 900, "items": [
            {"state": "ok", "id": IMAGE, "url": "https://s3.scnative.dev/a"},
            {"state": "deleted", "id": THUMB},
            {"state": "lost", "id": CONVERSATION},
        ]}),
    );
    rig.server.json(
        "GET",
        "/chat/media/usage",
        json!({"bytes": 176_128, "reserved": 0, "quota": QUOTA, "files": 1, "conversations": [
            {"conversation_id": CONVERSATION, "bytes": 176_128, "files": 1, "gone": false},
        ]}),
    );
    rig.server.json(
        "GET",
        "/chat/media/mine",
        json!({"items": [{
            "id": IMAGE, "kind": "image", "conversation_id": CONVERSATION, "thumb_id": THUMB,
            "bytes": 176_128, "status": "attached", "chats": 2, "created_at": START_MS,
        }], "next_cursor": "c2"}),
    );
    rig.server.json(
        "POST",
        "/chat/media/delete",
        json!({"deleted": 1, "freed": 176_128, "used": 0, "quota": QUOTA}),
    );
    rig.server.json(
        "POST",
        "/chat/media/restore",
        json!({"restored": 1, "missed": 0, "used": 176_128, "quota": QUOTA}),
    );

    let reserve = ReserveMedia {
        upload_id: Uuid::from_u128(0x55),
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        bytes: 151_552,
        thumb_bytes: Some(24_576),
    };
    let reserved = api.reserve_media(&reserve).await.unwrap();
    assert_eq!(reserved.thumb.as_ref().unwrap().id, THUMB);
    assert_eq!(reserved.put.headers["content-length"], "151552");
    assert_eq!(api.commit_media(IMAGE).await.unwrap().bytes, 176_128);
    let urls = api
        .media_urls(&sc_chat::api::wire::UrlsRequest {
            conversation_id: Some(CONVERSATION),
            ids: vec![IMAGE, THUMB, CONVERSATION],
        })
        .await
        .unwrap();
    assert_eq!(
        urls.items,
        vec![
            MediaUrl::Ok {
                id: IMAGE,
                url: "https://s3.scnative.dev/a".into()
            },
            MediaUrl::Deleted { id: THUMB },
            MediaUrl::Lost { id: CONVERSATION },
        ]
    );
    assert_eq!(api.media_usage().await.unwrap().conversations.len(), 1);
    let mine = api
        .media_mine(&MineQuery {
            order: MineOrder::Big,
            conversation_id: Some(CONVERSATION),
            cursor: Some("c1".into()),
            limit: 50,
        })
        .await
        .unwrap();
    assert_eq!(mine.items[0].status, MyMediaStatus::Attached);
    assert_eq!(mine.next_cursor.as_deref(), Some("c2"));
    let deleted = api
        .delete_media(&MediaSelection::ids(vec![IMAGE]))
        .await
        .unwrap();
    assert_eq!(deleted.freed, 176_128);
    assert_eq!(
        api.restore_media(&MediaSelection::all())
            .await
            .unwrap()
            .restored,
        1
    );

    let calls = rig.server.calls();
    let body = |path: &str| {
        calls
            .iter()
            .find(|call| call.path == path)
            .map(|call| call.body.clone())
            .unwrap()
    };
    assert_eq!(
        body("/chat/media/reserve"),
        json!({
            "upload_id": Uuid::from_u128(0x55), "conversation_id": CONVERSATION,
            "kind": "image", "bytes": 151_552, "thumb_bytes": 24_576,
        })
    );
    assert_eq!(body(&commit), Value::Null);
    assert_eq!(
        body("/chat/media/urls"),
        json!({"conversation_id": CONVERSATION, "ids": [IMAGE, THUMB, CONVERSATION]})
    );
    assert_eq!(body("/chat/media/delete"), json!({"ids": [IMAGE]}));
    assert_eq!(body("/chat/media/restore"), json!({"all": true}));
    let mine_call = calls
        .iter()
        .find(|call| call.path == "/chat/media/mine")
        .unwrap();
    assert_eq!(
        mine_call.query,
        format!("order=big&conversation_id={CONVERSATION}&cursor=c1&limit=50")
    );
}

#[tokio::test]
async fn media_refusals_arrive_as_their_own_errors() {
    let rig = rig();
    let api = rig.engine.api();
    let reserve = ReserveMedia {
        upload_id: Uuid::new_v4(),
        conversation_id: CONVERSATION,
        kind: MediaKind::Avatar,
        bytes: 16_384,
        thumb_bytes: None,
    };
    for step in [
        error(
            403,
            json!({"error": "chat_quota_exceeded", "used": 1_000, "quota": QUOTA}),
        ),
        error(403, json!({"error": "chat_media_star_required"})),
        error(403, json!({"error": "chat_room_star_required"})),
        error(503, json!({"error": "chat_media_uploads_paused"})),
    ] {
        rig.server.on("POST", "/chat/media/reserve", step);
    }
    rig.server.on(
        "POST",
        &format!("/chat/media/{IMAGE}/commit"),
        error(
            409,
            json!({"error": "chat_media_missing", "ids": [THUMB, "not-a-uuid"]}),
        ),
    );

    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(api.reserve_media(&reserve).await.unwrap_err());
    }
    assert_eq!(
        seen,
        vec![
            ChatError::QuotaExceeded {
                used: 1_000,
                quota: QUOTA
            },
            ChatError::MediaStarRequired,
            ChatError::RoomStarRequired,
            ChatError::MediaUploadsPaused,
        ]
    );
    assert_eq!(
        api.commit_media(IMAGE).await.unwrap_err(),
        ChatError::MediaMissing { ids: vec![THUMB] }
    );
    assert_eq!(
        rig.server.calls_to("POST", "/chat/media/reserve")[0].body,
        json!({"upload_id": reserve.upload_id, "conversation_id": CONVERSATION, "kind": "avatar", "bytes": 16_384})
    );
}

#[test]
fn a_reserve_answer_for_an_avatar_has_no_thumb() {
    let answer: ReserveAnswer = serde_json::from_value(json!({
        "id": IMAGE, "put": {"url": "https://s3.scnative.dev/a"},
        "expires_in_s": 600, "used": 0, "quota": QUOTA,
    }))
    .unwrap();
    assert!(answer.thumb.is_none() && answer.put.headers.is_empty());
    assert!(
        serde_json::to_value(&answer)
            .unwrap()
            .get("thumb")
            .is_none()
    );
}
