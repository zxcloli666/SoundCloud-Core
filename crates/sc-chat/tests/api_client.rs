mod sync_support;

use std::sync::Arc;

use sc_chat::api::wire::{MyChange, SyncRequest};
use sc_chat::api::{BindSigner, SyncReply};
use sc_chat::crypto::DeviceKeys;
use sc_chat::{
    ChatError, HistoryQuery, LinkMode, LinkState, LoginState, LoginStep, Mute, MyPatch, Tier,
};
use serde_json::json;
use sync_support::{DEVICE, START_MS, Step, rig};
use uuid::Uuid;

fn config() -> serde_json::Value {
    json!({
        "enabled": true,
        "access": "beta",
        "send_paused": false,
        "hold_max_ms": 20000,
        "server_now": START_MS,
        "incarnation": Uuid::nil(),
        "ock_current": 1,
        "limits": {
            "text_max": 4096,
            "embeds_max": 3,
            "group_members_max": 200,
            "devices_max": 10,
            "envelope_max": 65536,
            "disappearing_ttls": [3600, 86400, 604800],
        },
    })
}

fn signer() -> BindSigner {
    BindSigner {
        device_id: DEVICE,
        keys: Arc::new(DeviceKeys::from_seeds([3; 32], [4; 32])),
        target_session: None,
    }
}

fn sync_request() -> SyncRequest {
    SyncRequest {
        pos: 0,
        inc: None,
        hold_ms: 0,
        max_bytes: Tier::Direct.max_bytes(),
        known: Vec::new(),
        focus: None,
        read: Vec::new(),
        delivered: Vec::new(),
    }
}

#[tokio::test]
async fn calls_fall_through_tiers_and_start_with_the_one_that_answered() {
    let rig = rig();
    let api = rig.engine.api();
    rig.server
        .on("GET", "/chat/config", Step::Fail(ChatError::Offline));
    let mut with_user = config();
    with_user["user_id"] = json!("100");
    rig.server.json("GET", "/chat/config", with_user);
    rig.server.json("GET", "/chat/config", config());

    let first = api.config().await.unwrap();
    assert!(first.enabled);
    assert_eq!(first.user_id.as_deref(), Some("100"));
    assert_eq!(api.config().await.unwrap().user_id, None);

    let tiers: Vec<Tier> = rig
        .server
        .calls_to("GET", "/chat/config")
        .iter()
        .map(|call| call.tier)
        .collect();
    assert_eq!(tiers, vec![Tier::Direct, Tier::Relay, Tier::Relay]);
}

#[tokio::test]
async fn every_tier_down_is_offline() {
    let rig = rig();
    for _ in 0..3 {
        rig.server
            .on("GET", "/chat/config", Step::Fail(ChatError::Offline));
    }
    assert_eq!(rig.engine.api().config().await, Err(ChatError::Offline));
}

#[tokio::test]
async fn a_clock_error_moves_the_channel_clock_and_retries_once() {
    let rig = rig();
    let server_now = START_MS + 600_000;
    rig.server.on(
        "GET",
        "/chat/config",
        Step::Json(
            400,
            json!({"error": "chat_clock", "server_now": server_now}),
        ),
    );
    rig.server.json("GET", "/chat/config", config());

    rig.engine.api().config().await.unwrap();

    assert_eq!(rig.engine.api().channel().now_ms(), server_now);
    assert_eq!(rig.server.calls_to("GET", "/chat/config").len(), 2);
}

#[tokio::test]
async fn a_replayed_request_is_sent_again_once() {
    let rig = rig();
    rig.server.on(
        "GET",
        "/chat/config",
        Step::Json(409, json!({"error": "chat_replay"})),
    );
    rig.server.on(
        "GET",
        "/chat/config",
        Step::Json(409, json!({"error": "chat_replay"})),
    );
    assert_eq!(rig.engine.api().config().await, Err(ChatError::Replay));
    assert_eq!(rig.server.calls_to("GET", "/chat/config").len(), 2);
}

#[tokio::test]
async fn errors_without_a_known_code_map_by_status() {
    let rig = rig();
    let api = rig.engine.api();
    rig.server
        .on("GET", "/chat/config", Step::Json(404, json!({})));
    rig.server.on(
        "GET",
        "/chat/config",
        Step::Json(503, json!({"error": "odd"})),
    );
    rig.server
        .on("GET", "/chat/config", Step::Json(500, json!(null)));
    rig.server.on(
        "GET",
        "/chat/config",
        Step::Json(200, json!({"enabled": "maybe"})),
    );
    assert_eq!(api.config().await, Err(ChatError::NotFound));
    assert_eq!(api.config().await, Err(ChatError::Busy { retry_ms: 0 }));
    assert_eq!(api.config().await, Err(ChatError::Internal));
    assert_eq!(api.config().await, Err(ChatError::Protocol));
}

#[tokio::test]
async fn sync_tells_idle_from_later_and_parks_only_with_a_hold() {
    let rig = rig();
    let api = rig.engine.api();
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    rig.server.on(
        "POST",
        "/chat/sync",
        Step::Empty(204, vec![("Retry-After-Ms".into(), "1234".into())]),
    );
    let mut parked = sync_request();
    parked.hold_ms = 15_000;

    assert_eq!(
        api.sync(&sync_request(), Tier::Pro).await,
        Ok(SyncReply::Idle)
    );
    assert_eq!(
        api.sync(&parked, Tier::Pro).await,
        Ok(SyncReply::Later { retry_ms: 1234 })
    );

    let calls = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!((calls[0].long_poll, calls[0].timeout_ms), (false, 15_000));
    assert_eq!((calls[1].long_poll, calls[1].timeout_ms), (true, 30_000));
    assert!(calls.iter().all(|call| call.tier == Tier::Pro));
}

#[tokio::test]
async fn paths_and_queries_are_encoded() {
    let rig = rig();
    let api = rig.engine.api();
    let id = Uuid::from_u128(7);
    rig.server.json(
        "GET",
        "/chat/users/a%20b%2Fc/card",
        sync_support::fixtures::card("a b/c", "x"),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{id}/messages"),
        json!({"messages": [], "more": false}),
    );
    rig.server.on(
        "DELETE",
        &format!("/chat/conversations/{id}/members/9"),
        Step::Empty(204, vec![]),
    );
    rig.server.on(
        "DELETE",
        &format!("/chat/conversations/{id}"),
        Step::Empty(204, vec![]),
    );

    api.user_card("a b/c").await.unwrap();
    let page = HistoryQuery {
        before: Some(40),
        after: None,
        limit: 20,
    };
    api.history(id, &page).await.unwrap();
    api.remove_member(id, "9", true).await.unwrap();
    api.delete_conversation(id, sc_chat::DeleteScope::Everyone)
        .await
        .unwrap();

    let calls = rig.server.calls();
    assert_eq!(calls[1].query, "before=40&limit=20");
    assert_eq!(calls[2].query, "ban=true");
    assert_eq!(calls[3].query, "for=everyone");
}

#[tokio::test]
async fn sealed_auth_calls_sign_with_the_bind_and_map_states() {
    let rig = rig();
    let api = rig.engine.api();
    rig.server.json(
        "GET",
        "/auth/login",
        json!({"url": "https://soundcloud.com/connect", "loginRequestId": "l1"}),
    );
    rig.server.json(
        "GET",
        "/auth/login/status",
        json!({"status": "processing", "step": "extract"}),
    );
    rig.server.json(
        "POST",
        "/auth/link/create",
        json!({"linkRequestId": "k1", "claimToken": "t", "expiresAt": START_MS, "mode": "pull"}),
    );
    rig.server.json(
        "GET",
        "/auth/link/status",
        json!({"status": "claimed", "mode": "pull", "sessionId": "s2"}),
    );

    assert_eq!(
        api.login_begin(&signer()).await.unwrap().login_request_id,
        "l1"
    );
    let status = api.login_status(&signer(), "l1").await.unwrap();
    assert_eq!(
        (status.status, status.step),
        (LoginState::Pending, Some(LoginStep::Extract))
    );
    assert_eq!(
        api.link_create_pull(&signer()).await.unwrap().mode,
        LinkMode::Pull
    );
    let link = api.link_status(&signer(), "k1").await.unwrap();
    assert_eq!(
        (link.status, link.session_id.as_deref()),
        (LinkState::Claimed, Some("s2"))
    );

    let calls = rig.server.calls();
    assert!(calls.iter().all(|call| call.bound));
    assert_eq!(calls[1].query, "id=l1");
    assert_eq!(calls[2].body, json!({"mode": "pull"}));
}

#[tokio::test]
async fn device_calls_need_an_identity() {
    let rig = rig();
    rig.engine.api().set_identity(None);
    assert_eq!(rig.engine.api().devices().await, Err(ChatError::NotReady));
    assert!(rig.server.calls().is_empty());
}

#[test]
fn my_changes_send_only_what_changed() {
    let forever = MyChange::from(&MyPatch {
        mute: Some(Mute::Forever),
        notify: None,
        pinned: Some(true),
        archived: None,
    });
    assert_eq!(
        serde_json::to_value(&forever).unwrap(),
        json!({"muted_until": 253_402_300_799_000_i64, "pinned": true})
    );
    let unmute = MyChange::from(&MyPatch {
        mute: Some(Mute::Off),
        notify: None,
        pinned: None,
        archived: None,
    });
    assert_eq!(
        serde_json::to_value(&unmute).unwrap(),
        json!({"muted_until": null})
    );
}

#[test]
fn wire_views_turn_into_what_the_webview_shows() {
    use sc_chat::api::wire::{CardView, InviteView, RoomPreviewView};
    use sc_chat::{Invite, RoomPreview, UserCard};

    let invite: InviteView = serde_json::from_value(json!({
        "token": "tok-1", "conversation_id": Uuid::nil(), "expires_at": 5, "uses_left": null,
    }))
    .unwrap();
    assert_eq!(Invite::from(&invite).url, "scd://invite?token=tok-1");

    let card: CardView =
        serde_json::from_value(sync_support::fixtures::card("7", "seven")).unwrap();
    let mut closed = card.clone();
    closed.can_message = "unavailable".into();
    assert!(UserCard::from(&card).can_message);
    assert!(!UserCard::from(&closed).can_message);
    assert_eq!(UserCard::from(&card).user.name, "seven");

    let preview: RoomPreviewView = serde_json::from_value(json!({
        "code": "ABC234",
        "title": null,
        "host": {"id": "1", "name": "Host", "avatar_url": null},
        "track": {"urn": "soundcloud:tracks:5", "title": "Song", "artist": null, "artwork_url": "http://x.com/a.jpg", "duration_ms": 1000},
        "listeners": 2,
        "capacity": 10,
        "public": true,
        "alive": true,
    }))
    .unwrap();
    let shown = RoomPreview::from(&preview);
    let track = shown.track.unwrap();
    assert_eq!((track.title.as_str(), track.artwork_url), ("Song", None));
    assert_eq!(shown.host.name, "Host");
}
