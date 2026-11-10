mod sync_support;

use sc_chat::api::wire::{ConversationList, LogPage, PublicView};
use sc_chat::envelope::post_body_len;
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::{
    CAP_PUBLIC_CHAT, ChatError, ConversationKind, MessageClass, ModeratorMute, Newcomer,
    caps_with_public_chat,
};
use serde_json::json;
use sync_support::fixtures::{answer, entry, post, post_from, summary, text, view};
use sync_support::real::{INC, direct_plan, real};
use sync_support::{ME, START_MS, Step, rig};
use uuid::Uuid;

const PUBLIC: Uuid = Uuid::from_u128(0x7075626c6963);

fn public_view(pinged: Option<bool>) -> serde_json::Value {
    let mut public = view(PUBLIC, 3);
    public["kind"] = json!("public");
    public["peer"] = serde_json::Value::Null;
    public["settings"] = json!({"admins_only": false, "history_visible": true, "slow_mode_s": 30});
    if let Some(pinged) = pinged {
        public["pinged"] = json!(pinged);
    }
    public
}

#[test]
fn an_unknown_kind_still_parses_in_a_list() {
    let mut odd = view(Uuid::from_u128(2), 1);
    odd["kind"] = json!("channel_v2");
    let list: ConversationList = serde_json::from_value(json!({
        "items": [view(Uuid::from_u128(1), 1), public_view(None), odd],
        "next_cursor": null,
    }))
    .unwrap();
    let kinds: Vec<_> = list.items.iter().map(|item| item.kind).collect();
    assert_eq!(
        kinds,
        [
            ConversationKind::Direct,
            ConversationKind::Public,
            ConversationKind::Unknown
        ]
    );
    assert!(!list.items[1].pinged);
    assert_eq!(list.items[1].slow_mode_s(), 30);
}

#[tokio::test]
async fn an_unknown_kind_in_a_view_and_in_a_sync_answer_does_not_break_the_engine() {
    let rig = real(&[]);
    let id = Uuid::from_u128(5);
    let mut odd = view(id, 2);
    odd["peer"] = serde_json::Value::Null;
    odd["kind"] = json!("channel_v2");
    rig.server
        .json("GET", &format!("/chat/conversations/{id}"), odd.clone());
    let fetched = rig.engine.api().conversation(id).await.unwrap();
    assert_eq!(fetched.kind, ConversationKind::Unknown);

    rig.server
        .json("GET", &format!("/chat/conversations/{id}"), odd);
    let summary = rig.engine.refresh_conversation(id).await.unwrap();
    assert_eq!(summary.kind, ConversationKind::Unknown);
    let stored = rig.engine.store().conversation(id).unwrap().unwrap();
    assert_eq!(stored.kind, ConversationKind::Unknown);

    let answer = sync_support::fixtures::answer(
        2,
        Uuid::nil(),
        vec![sync_support::fixtures::entry(id, 2, Vec::new())],
    );
    let parsed: sc_chat::api::wire::SyncAnswer = serde_json::from_value(answer).unwrap();
    assert_eq!(parsed.convs[0].id, id);
}

#[tokio::test]
async fn opening_the_public_chat_keeps_online_pings_and_slow_mode() {
    let rig = real(&[]);
    rig.server.json(
        "POST",
        "/chat/public/open",
        json!({"conversation": public_view(Some(true)), "online": 41}),
    );

    let opened = rig.engine.open_public().await.unwrap();
    let summary = opened.conversation;

    assert_eq!(opened.online, 41);
    assert_eq!((opened.mute, opened.newcomer), (None, None));
    assert_eq!(summary.kind, ConversationKind::Public);
    assert!(summary.pinged);
    assert_eq!(summary.slow_mode_s, 30);
    assert!(!summary.history_locked);
    let stored = rig.engine.store().conversation(PUBLIC).unwrap().unwrap();
    assert!(stored.pinged);
    assert_eq!(stored.slow_mode_s, 30);
    let parsed: PublicView =
        serde_json::from_value(json!({"conversation": public_view(None), "online": 1})).unwrap();
    assert_eq!(parsed.online, 1);
}

#[tokio::test]
async fn public_calls_use_the_server_routes_and_bodies() {
    let rig = rig();
    let api = rig.engine.api();
    let log_id = Uuid::from_u128(77);
    rig.server
        .on("POST", "/chat/public/watch", Step::Empty(204, vec![]));
    rig.server
        .on("DELETE", "/chat/public/watch", Step::Empty(204, vec![]));
    rig.server.on(
        "PUT",
        "/chat/public/moderators/42",
        Step::Empty(204, vec![]),
    );
    rig.server
        .json("PUT", "/chat/public/mutes/42", json!({"until": 5000}));
    rig.server
        .on("DELETE", "/chat/public/mutes/42", Step::Empty(204, vec![]));
    rig.server
        .on("PUT", "/chat/public/bans/42", Step::Empty(204, vec![]));
    rig.server
        .on("PUT", "/chat/public/bans/42", Step::Empty(204, vec![]));
    rig.server
        .on("DELETE", "/chat/public/bans/42", Step::Empty(204, vec![]));
    rig.server
        .on("PUT", "/chat/public/slow-mode", Step::Empty(204, vec![]));
    rig.server.json(
        "GET",
        "/chat/public/log",
        json!({
            "entries": [{
                "id": log_id, "by": "9", "action": "mute", "target": "user:42",
                "detail": {"until": 5000, "seconds": 600, "reason": null}, "ts": START_MS,
            }],
            "next": null,
        }),
    );

    api.public_watch().await.unwrap();
    api.public_unwatch().await.unwrap();
    api.public_moderator("42", true).await.unwrap();
    let muted = api.public_mute("42", 600, Some("  spam ")).await.unwrap();
    assert_eq!(muted.until, 5000);
    api.public_mute("42", 3600, None).await.ok();
    api.public_unmute("42").await.unwrap();
    api.public_ban("42", None).await.unwrap();
    api.public_ban("42", Some("raid")).await.unwrap();
    api.public_unban("42").await.unwrap();
    api.public_slow_mode(30).await.unwrap();
    let page: LogPage = api.public_log(Some(log_id), Some(20)).await.unwrap();
    assert_eq!(page.entries[0].action, "mute");
    assert_eq!(page.next, None);

    let body =
        |method: &str, path: &str, n: usize| rig.server.calls_to(method, path)[n].body.clone();
    assert_eq!(
        body("PUT", "/chat/public/moderators/42", 0),
        json!({"on": true})
    );
    assert_eq!(
        body("PUT", "/chat/public/mutes/42", 0),
        json!({"seconds": 600, "reason": "spam"})
    );
    assert_eq!(
        body("PUT", "/chat/public/mutes/42", 1),
        json!({"seconds": 3600})
    );
    assert_eq!(body("PUT", "/chat/public/bans/42", 0), json!({}));
    assert_eq!(
        body("PUT", "/chat/public/bans/42", 1),
        json!({"reason": "raid"})
    );
    assert_eq!(
        body("PUT", "/chat/public/slow-mode", 0),
        json!({"seconds": 30})
    );
    let log = &rig.server.calls_to("GET", "/chat/public/log")[0];
    assert_eq!(log.query, format!("before={log_id}&limit=20"));
}

#[tokio::test]
async fn public_calls_refuse_values_the_server_would_refuse() {
    let rig = rig();
    let api = rig.engine.api();
    assert_eq!(
        api.public_mute("42", 60, None).await,
        Err(ChatError::Invalid)
    );
    let long = "x".repeat(201);
    assert_eq!(
        api.public_ban("42", Some(&long)).await,
        Err(ChatError::Invalid)
    );
    assert_eq!(api.public_slow_mode(601).await, Err(ChatError::Invalid));
    assert!(rig.server.calls().is_empty());
}

#[tokio::test]
async fn public_errors_map_to_their_shapes() {
    let rig = rig();
    let api = rig.engine.api();
    for body in [
        json!({"error": "chat_muted", "until": 9000}),
        json!({"error": "chat_slow_mode", "retry_ms": 4000}),
        json!({"error": "chat_not_found"}),
        json!({"error": "chat_disabled"}),
    ] {
        rig.server
            .on("POST", "/chat/public/open", Step::Json(403, body));
    }
    assert_eq!(
        api.open_public().await,
        Err(ChatError::Muted { until: 9000 })
    );
    assert_eq!(
        api.open_public().await,
        Err(ChatError::SlowMode { retry_ms: 4000 })
    );
    assert_eq!(api.open_public().await, Err(ChatError::NotFound));
    assert_eq!(api.open_public().await, Err(ChatError::Disabled));
}

#[tokio::test]
async fn a_device_refresh_sends_the_version_and_every_cap() {
    let rig = rig();
    let caps = caps_with_public_chat(vec!["embeds".into()]);
    assert_eq!(caps, ["embeds", CAP_PUBLIC_CHAT]);
    assert_eq!(caps_with_public_chat(caps.clone()), caps);
    rig.server.json(
        "PUT",
        "/chat/devices/me",
        sync_support::fixtures::device(Uuid::from_u128(1), START_MS, true, true),
    );

    rig.engine
        .api()
        .refresh_device("9.0.0", &caps)
        .await
        .unwrap();

    let sent = &rig.server.calls_to("PUT", "/chat/devices/me")[0];
    assert_eq!(
        sent.body,
        json!({"app_version": "9.0.0", "caps": ["embeds", "public_chat"]})
    );
}

#[tokio::test]
async fn a_public_post_names_its_mentions_as_pings_and_other_chats_do_not() {
    let other = Uuid::from_u128(0xb);
    let mut room = summary(PUBLIC, 0);
    room.kind = ConversationKind::Public;
    let direct = summary(other, 0);
    let rig = real(&[room, direct]);
    rig.serve_keys(PUBLIC, &[1]);
    rig.serve_keys(other, &[1]);
    let mut mentioning = text("hi all");
    mentioning.mentions = ["1", "2", "1", "3", "4", "5", "6", "7"]
        .map(String::from)
        .to_vec();
    let outbox = Outbox::new(rig.engine.store().clone());
    for (client, conversation) in [(0xc1, PUBLIC), (0xc2, other)] {
        outbox
            .push(OutboxItem {
                client_id: Uuid::from_u128(client),
                conversation_id: conversation,
                class: MessageClass::Post,
                target_seq: None,
                envelope: mentioning.encode_to_vec(),
                created_at: START_MS,
                attempts: 0,
                next_at: START_MS,
            })
            .unwrap();
        rig.server.json(
            "POST",
            &format!("/chat/conversations/{conversation}/messages"),
            json!({"seq": 1, "ts": START_MS, "expires_at": null}),
        );
    }

    rig.engine.flush_outbox().await.unwrap();

    let sent = rig
        .server
        .calls_to("POST", &format!("/chat/conversations/{PUBLIC}/messages"));
    assert_eq!(sent[0].body["pings"], json!(["1", "2", "3", "4", "5"]));
    let plain = rig
        .server
        .calls_to("POST", &format!("/chat/conversations/{other}/messages"));
    assert!(plain[0].body.get("pings").is_none());
}

#[tokio::test]
async fn a_public_reply_pings_the_author_it_answers_ahead_of_the_mentions() {
    let mut room = summary(PUBLIC, 0);
    room.kind = ConversationKind::Public;
    let rig = real(&[room]);
    rig.serve_keys(PUBLIC, &[1]);
    let heard = vec![
        post_from(PUBLIC, 1, 1, "7", "anyone here"),
        post_from(PUBLIC, 2, 1, ME, "me"),
    ];
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(2, INC, vec![entry(PUBLIC, 2, heard)]),
    );
    rig.server.json(
        "GET",
        &format!("/chat/conversations/{PUBLIC}"),
        public_view(None),
    );
    rig.engine.step(direct_plan()).await.unwrap();
    let send = format!("/chat/conversations/{PUBLIC}/messages");
    let outbox = Outbox::new(rig.engine.store().clone());
    for (client, reply_to) in [(0xc1, 1), (0xc2, 2), (0xc3, 9)] {
        let mut reply = text("yes");
        reply.reply_to = reply_to;
        reply.mentions = vec!["3".into(), "7".into()];
        outbox
            .push(OutboxItem {
                client_id: Uuid::from_u128(client),
                conversation_id: PUBLIC,
                class: MessageClass::Post,
                target_seq: None,
                envelope: reply.encode_to_vec(),
                created_at: START_MS,
                attempts: 0,
                next_at: START_MS,
            })
            .unwrap();
        rig.server.json(
            "POST",
            &send,
            json!({"seq": 2 + client - 0xc0, "ts": START_MS, "expires_at": null}),
        );
    }

    assert_eq!(rig.engine.flush_outbox().await, Ok(3));

    let sent = rig.server.calls_to("POST", &send);
    assert_eq!(sent[0].body["pings"], json!(["7", "3"]));
    assert_eq!(sent[1].body["pings"], json!(["3", "7"]));
    assert_eq!(sent[2].body["pings"], json!(["3", "7"]));
}

#[tokio::test]
async fn a_public_entry_with_messages_rereads_the_view_unless_the_chat_is_watched() {
    let mut room = summary(PUBLIC, 0);
    room.kind = ConversationKind::Public;
    let rig = real(&[room]);
    rig.serve_keys(PUBLIC, &[1]);
    let path = format!("/chat/conversations/{PUBLIC}");
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            2,
            INC,
            vec![entry(PUBLIC, 1, vec![post(PUBLIC, 1, 1, "you")])],
        ),
    );
    rig.server.json("GET", &path, public_view(Some(true)));

    rig.engine.step(direct_plan()).await.unwrap();

    assert!(
        rig.engine
            .store()
            .conversation(PUBLIC)
            .unwrap()
            .unwrap()
            .pinged
    );
    assert_eq!(rig.server.calls_to("GET", &path).len(), 1);

    rig.engine.set_visible(true);
    rig.engine.set_focus(Some(PUBLIC));
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            3,
            INC,
            vec![entry(PUBLIC, 2, vec![post(PUBLIC, 2, 1, "more")])],
        ),
    );

    rig.engine.step(direct_plan()).await.unwrap();

    assert_eq!(rig.server.calls_to("GET", &path).len(), 1);
    let stored = rig.engine.store().conversation(PUBLIC).unwrap().unwrap();
    assert_eq!(stored.last_seq, 2);
}

#[tokio::test]
async fn opening_the_public_chat_carries_the_moderator_mute_and_the_newcomer_window() {
    let rig = real(&[]);
    rig.server.json(
        "POST",
        "/chat/public/open",
        json!({
            "conversation": public_view(None),
            "online": 3,
            "mute": {"until": START_MS + 600_000, "reason": "spam"},
            "newcomer": {"until": null, "body_max": 512},
        }),
    );
    rig.server.json(
        "POST",
        "/chat/public/open",
        json!({
            "conversation": public_view(None),
            "online": 4,
            "mute": null,
            "newcomer": {"until": START_MS + 300_000, "body_max": 512},
        }),
    );

    let opened = rig.engine.open_public().await.unwrap();
    assert_eq!(
        opened.mute,
        Some(ModeratorMute {
            until: START_MS + 600_000,
            reason: Some("spam".into()),
        })
    );
    assert_eq!(
        opened.newcomer,
        Some(Newcomer {
            until: None,
            body_max: 512,
        })
    );
    let again = rig.engine.open_public().await.unwrap();
    assert_eq!(again.mute, None);
    assert_eq!(again.newcomer.unwrap().until, Some(START_MS + 300_000));

    let parsed: PublicView = serde_json::from_value(json!({
        "conversation": public_view(None),
        "online": 1,
        "mute": {"until": 5},
    }))
    .unwrap();
    assert_eq!(parsed.mute.unwrap().reason, None);
    assert_eq!(parsed.newcomer, None);
}

#[test]
fn a_newcomer_text_fits_by_its_padded_body() {
    assert_eq!(post_body_len("hi"), 256);
    assert_eq!(post_body_len(&"x".repeat(300)), 512);
    assert_eq!(post_body_len(""), 0);
    let window = Newcomer {
        until: None,
        body_max: 256,
    };
    assert!(window.fits("hi"));
    assert!(!window.fits(&"x".repeat(300)));
    let roomy = Newcomer {
        until: Some(START_MS),
        body_max: 512,
    };
    assert!(roomy.fits(&"x".repeat(300)));
}

#[tokio::test]
async fn a_public_entry_with_a_known_rev_keeps_the_view_it_has() {
    let mut room = summary(PUBLIC, 0);
    room.kind = ConversationKind::Public;
    let rig = real(&[room]);
    rig.serve_keys(PUBLIC, &[1]);
    let path = format!("/chat/conversations/{PUBLIC}");
    for (pos, seq, rev) in [(2, 1, 5), (3, 2, 5), (4, 3, 6)] {
        let mut heard = entry(PUBLIC, seq, vec![post(PUBLIC, seq, 1, "hi")]);
        heard["rev"] = json!(rev);
        rig.server
            .json("POST", "/chat/sync", answer(pos, INC, vec![heard]));
        rig.server.json("GET", &path, public_view(None));
        rig.engine.step(direct_plan()).await.unwrap();
    }

    assert_eq!(rig.server.calls_to("GET", &path).len(), 2);
    let stored = rig.engine.store().conversation(PUBLIC).unwrap().unwrap();
    assert_eq!(stored.last_seq, 3);
}
