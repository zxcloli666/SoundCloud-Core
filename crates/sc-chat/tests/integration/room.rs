use std::time::Duration;

use sc_chat::{ChatError, ConvId, RoomModeration, RoomSettingsPatch, SystemKind};
use serde_json::{Value, json};

use crate::support::http::Reply;
use crate::support::people::now_ms;
use crate::support::{Client, Stack, within};

const PURGED_WITHIN: Duration = Duration::from_secs(120);

fn ok(reply: &Reply) -> Value {
    assert!(
        reply.status < 300,
        "{}: {}",
        reply.status,
        String::from_utf8_lossy(&reply.body)
    );
    reply.json()
}

async fn join(stack: &Stack, client: &Client, code: &str) -> Reply {
    stack
        .as_session(
            client.person.session_id,
            "POST",
            &format!("/rooms/{code}/members"),
            Some(json!({"name": client.person.name})),
        )
        .await
}

async fn poll(stack: &Stack, client: &Client, code: &str) -> Reply {
    stack
        .as_session(
            client.person.session_id,
            "GET",
            &format!("/rooms/{code}"),
            None,
        )
        .await
}

async fn leave(stack: &Stack, client: &Client, code: &str) {
    ok(&stack
        .as_session(
            client.person.session_id,
            "DELETE",
            &format!("/rooms/{code}/members/me"),
            None,
        )
        .await);
}

async fn seat(client: &Client, code: &str) -> ConvId {
    let room = client.api.room_chat(code).await.unwrap();
    client
        .engine
        .adopt_room(room.conversation_id, code, room.from_seq)
        .await
        .unwrap();
    room.conversation_id
}

async fn purged_locally(client: &Client, chat: ConvId) {
    client
        .sync_until("the room chat leaves the device", |client| {
            client.conversation(chat).is_none() && client.messages(chat).is_empty()
        })
        .await;
    assert!(client.events.gone().contains(&chat));
}

fn server_rows(stack: &Stack, chat: ConvId) -> i64 {
    stack.count(&format!(
        "SELECT (SELECT count(*) FROM chat.conversations WHERE id = '{chat}') + (SELECT count(*) FROM chat.messages WHERE conversation_id = '{chat}') + (SELECT count(*) FROM chat.conversation_keys WHERE conversation_id = '{chat}') + (SELECT count(*) FROM chat.members WHERE conversation_id = '{chat}')"
    ))
}

#[tokio::test]
#[ignore]
async fn a_room_chat_follows_moderation_and_handoff_and_is_purged_everywhere_when_the_room_ends() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    let cid = Client::up(&stack, &stack.person("cid")).await;
    let created = ok(&stack
        .as_session(
            ann.person.session_id,
            "POST",
            "/rooms",
            Some(json!({"name": "ann", "public": false})),
        )
        .await);
    let code = created["code"].as_str().unwrap().to_owned();
    for guest in [&bob, &cid] {
        ok(&join(&stack, guest, &code).await);
    }
    let chat = seat(&ann, &code).await;
    for guest in [&bob, &cid] {
        assert_eq!(seat(guest, &code).await, chat);
    }

    let renamed = RoomSettingsPatch {
        title: Some("Night set".into()),
        ..RoomSettingsPatch::default()
    };
    ann.api.room_settings(&code, &renamed).await.unwrap();
    assert_eq!(ok(&poll(&stack, &bob, &code).await)["title"], "Night set");
    assert_eq!(ann.send(chat, "welcome").await, Ok(1));
    for guest in [&bob, &cid] {
        guest
            .sync_until("a guest sees the rename and the first post", |guest| {
                guest.texts(chat).iter().any(|text| text == "welcome")
                    && guest.systems(chat).iter().any(|event| {
                        event.kind == SystemKind::Renamed
                            && event.name.as_deref() == Some("Night set")
                    })
            })
            .await;
    }

    let cid_id = cid.person.user_id.clone();
    let mute = RoomModeration::Mute {
        user_id: cid_id.clone(),
        seconds: 300,
    };
    ann.api.room_moderate(&code, &mute).await.unwrap();
    assert!(matches!(
        cid.raw_room_post(chat).await,
        Err(ChatError::Muted { .. })
    ));
    bob.sync_until("bob sees cid muted", |bob| {
        bob.has_system(chat, SystemKind::Muted, &cid_id)
    })
    .await;

    let bob_id = bob.person.user_id.clone();
    let kick = RoomModeration::Kick {
        user_id: bob_id.clone(),
    };
    ann.api.room_moderate(&code, &kick).await.unwrap();
    let gone = poll(&stack, &bob, &code).await;
    assert_eq!(
        (gone.status, gone.json()["code"].clone()),
        (404, json!("room_not_member"))
    );
    purged_locally(&bob, chat).await;
    ann.sync_until("the host sees the kick", |ann| {
        ann.has_system(chat, SystemKind::Kicked, &bob_id)
    })
    .await;

    ok(&join(&stack, &bob, &code).await);
    assert_eq!(seat(&bob, &code).await, chat);
    assert!(!bob.texts(chat).iter().any(|text| text == "welcome"));
    let ban = RoomModeration::Ban {
        user_id: bob_id.clone(),
    };
    ann.api.room_moderate(&code, &ban).await.unwrap();
    let refused = join(&stack, &bob, &code).await;
    assert_eq!(refused.status, 403);
    assert_eq!(refused.json()["code"], "room_banned");
    purged_locally(&bob, chat).await;
    cid.sync_until("cid sees the ban", |cid| {
        cid.has_system(chat, SystemKind::Banned, &bob_id)
    })
    .await;

    ok(&poll(&stack, &cid, &code).await);
    leave(&stack, &ann, &code).await;
    let handed = ok(&poll(&stack, &cid, &code).await);
    assert_eq!(handed["hostId"], cid_id.as_str());
    assert_eq!(handed["code"], code.as_str());
    assert_eq!(
        cid.api.room_chat(&code).await.unwrap().conversation_id,
        chat
    );
    cid.sync_until("cid sees the handoff", |cid| {
        cid.systems(chat).iter().any(|event| {
            event.kind == SystemKind::Handoff
                && [event.actor.as_deref(), event.target.as_deref()]
                    .contains(&Some(cid_id.as_str()))
        })
    })
    .await;
    purged_locally(&ann, chat).await;

    leave(&stack, &cid, &code).await;
    assert_eq!(poll(&stack, &cid, &code).await.status, 404);
    purged_locally(&cid, chat).await;
    within(
        "the server purges the ended room chat",
        PURGED_WITHIN,
        || async { server_rows(&stack, chat) == 0 },
    )
    .await;
}

const AGE_SCORES: &str = "for _, member in ipairs(redis.call('ZRANGE', KEYS[1], 0, -1)) do redis.call('ZADD', KEYS[1], ARGV[1], member) end return 1";
const ABANDONED_MS: i64 = 11 * 60 * 1_000;

#[tokio::test]
#[ignore]
async fn an_abandoned_room_is_reaped_after_ten_quiet_minutes_and_its_chat_leaves_every_device() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    let created = ok(&stack
        .as_session(
            ann.person.session_id,
            "POST",
            "/rooms",
            Some(json!({"name": "ann", "public": false})),
        )
        .await);
    let code = created["code"].as_str().unwrap().to_owned();
    ok(&join(&stack, &bob, &code).await);
    let chat = seat(&ann, &code).await;
    assert_eq!(seat(&bob, &code).await, chat);
    assert_eq!(ann.send(chat, "anyone here").await, Ok(1));
    bob.sync_until("bob reads the room", |bob| {
        bob.texts(chat).iter().any(|text| text == "anyone here")
    })
    .await;

    let quiet_since = (now_ms() as i64 - ABANDONED_MS).to_string();
    stack.redis(&["ZADD", "rooms:alive", &quiet_since, &code]);
    stack.redis(&[
        "EVAL",
        AGE_SCORES,
        "1",
        &format!("rooms:{code}:seen"),
        &quiet_since,
    ]);
    within("the reaper ends the quiet room", PURGED_WITHIN, || async {
        stack.redis(&["EXISTS", &format!("rooms:{code}")]) == "0"
    })
    .await;
    for client in [&ann, &bob] {
        purged_locally(client, chat).await;
    }
    within(
        "the server purges the reaped room chat",
        PURGED_WITHIN,
        || async { server_rows(&stack, chat) == 0 },
    )
    .await;
}
