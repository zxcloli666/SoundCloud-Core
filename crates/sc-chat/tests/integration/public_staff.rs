use sc_chat::{ChatError, DeliveryState, HistoryQuery, MemberRole, MemberState, ModeratorMute};

use crate::public::{PUBLIC, last_state, line, owner, seated, slow_mode};
use crate::support::{Stack, public_turn};

#[tokio::test]
#[ignore]
async fn a_moderator_named_by_the_owner_mutes_and_bans_and_the_log_lists_it() {
    let _turn = public_turn().await;
    let stack = Stack::from_env();
    slow_mode(&stack, 0);
    let boss = owner(&stack).await;
    let cid = seated(&stack, "cid").await;
    let dan = seated(&stack, "dan").await;
    let (cid_id, dan_id) = (cid.person.user_id.clone(), dan.person.user_id.clone());

    assert_eq!(
        cid.api.public_mute(&dan_id, 600, None).await.err(),
        Some(ChatError::NotMember)
    );
    assert_eq!(
        cid.api.public_moderator(&dan_id, true).await.err(),
        Some(ChatError::NotMember)
    );
    boss.api.public_moderator(&cid_id, true).await.unwrap();
    let promoted = cid.engine.open_public().await.unwrap();
    assert_eq!(promoted.conversation.role, MemberRole::Admin);
    assert_eq!(promoted.newcomer, None);
    assert_eq!(
        cid.api
            .public_mute(&boss.person.user_id, 600, None)
            .await
            .err(),
        Some(ChatError::NotMember)
    );

    let muted = cid
        .api
        .public_mute(&dan_id, 600, Some(" spam "))
        .await
        .unwrap();
    assert_eq!(
        dan.engine.open_public().await.unwrap().mute,
        Some(ModeratorMute {
            until: muted.until,
            reason: Some("spam".into()),
        })
    );
    assert_eq!(
        dan.raw_room_post(PUBLIC).await.err(),
        Some(ChatError::Muted { until: muted.until })
    );
    assert_eq!(dan.send(PUBLIC, &line(&dan, "while muted")).await, Ok(0));
    assert_eq!(last_state(&dan), Some(DeliveryState::Failed));
    cid.api.public_unmute(&dan_id).await.unwrap();
    assert_eq!(dan.engine.open_public().await.unwrap().mute, None);
    let free = line(&dan, "after the mute");
    assert_eq!(dan.send(PUBLIC, &free).await, Ok(1));

    cid.api.public_ban(&dan_id, Some("rude")).await.unwrap();
    let recent = HistoryQuery {
        before: None,
        after: None,
        limit: 20,
    };
    assert_eq!(
        dan.engine.fetch_history(PUBLIC, &recent).await.err(),
        Some(ChatError::NotMember)
    );
    assert_eq!(
        dan.api.public_watch().await.err(),
        Some(ChatError::NotMember)
    );
    assert_eq!(
        dan.raw_room_post(PUBLIC).await.err(),
        Some(ChatError::NotMember)
    );
    assert_eq!(dan.send(PUBLIC, &line(&dan, "while banned")).await, Ok(0));
    assert_eq!(last_state(&dan), Some(DeliveryState::Failed));
    cid.api.public_unban(&dan_id).await.unwrap();
    let back = dan.engine.open_public().await.unwrap();
    assert_eq!(back.conversation.state, MemberState::Active);
    let page = dan.engine.fetch_history(PUBLIC, &recent).await.unwrap();
    assert!(!page.messages.is_empty());
    assert_eq!(dan.send(PUBLIC, &line(&dan, "after the ban")).await, Ok(1));

    boss.api.public_slow_mode(0).await.unwrap();
    boss.api.public_moderator(&cid_id, false).await.unwrap();
    assert_eq!(
        cid.api.public_log(None, None).await.err(),
        Some(ChatError::NotMember)
    );
    boss.api.public_moderator(&cid_id, true).await.unwrap();
    let log = boss.api.public_log(None, Some(50)).await.unwrap();
    let told = |by: &str, action: &str, target: &str| {
        log.entries
            .iter()
            .find(|entry| entry.by == by && entry.action == action && entry.target == target)
            .cloned()
    };
    let dan_target = format!("user:{dan_id}");
    assert!(told(&boss.person.user_id, "moderator_add", &cid_id).is_some());
    assert!(told(&boss.person.user_id, "moderator_remove", &cid_id).is_some());
    let mute = told(&cid_id, "mute", &dan_target).expect("the mute is logged");
    assert_eq!(mute.detail["reason"], "spam");
    assert_eq!(mute.detail["seconds"], 600);
    assert_eq!(mute.detail["until"], muted.until);
    assert!(told(&cid_id, "unmute", &dan_target).is_some());
    let ban = told(&cid_id, "ban", &dan_target).expect("the ban is logged");
    assert_eq!(ban.detail["reason"], "rude");
    assert!(told(&cid_id, "unban", &dan_target).is_some());
    let slow = told(
        &boss.person.user_id,
        "slow_mode",
        &format!("conversation:{PUBLIC}"),
    );
    assert_eq!(slow.expect("slow mode is logged").detail["seconds"], 0);

    let own = cid.api.public_log(None, Some(50)).await.unwrap();
    assert!(!own.entries.is_empty());
    assert!(own.entries.iter().all(|entry| entry.by == cid_id));
    assert_eq!(
        dan.api.public_log(None, None).await.err(),
        Some(ChatError::NotMember)
    );
    boss.api.public_moderator(&cid_id, false).await.unwrap();
}
