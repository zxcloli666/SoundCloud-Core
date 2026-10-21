mod sync_support;

use sc_chat::store::SyncState;
use sc_chat::sync::{BACKOFF_MAX_MS, Local, SyncOutcome};
use sc_chat::{ChatError, Tier};
use serde_json::json;
use sync_support::{Step, rig, rig_on};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);

fn synced(rig: &sync_support::Rig) {
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
}

async fn step(rig: &sync_support::Rig) -> (Tier, u64, SyncOutcome) {
    let plan = rig.engine.next_plan().unwrap();
    let outcome = rig.engine.step(plan).await.unwrap();
    (plan.tier, plan.hold_ms, outcome)
}

fn retry(outcome: SyncOutcome) -> u64 {
    match outcome {
        SyncOutcome::Retry { after_ms } => after_ms,
        other => panic!("expected a retry, got {other:?}"),
    }
}

#[tokio::test]
async fn a_dead_tier_falls_through_at_once_then_backs_off() {
    let rig = rig_on(&[Tier::Direct, Tier::Relay]);
    synced(&rig);
    for _ in 0..4 {
        rig.server
            .on("POST", "/chat/sync", Step::Fail(ChatError::Offline));
    }
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));

    let (tier, _, outcome) = step(&rig).await;
    assert_eq!((tier, retry(outcome)), (Tier::Direct, 0));
    let (tier, _, outcome) = step(&rig).await;
    assert_eq!(tier, Tier::Relay);
    assert!((500..=1_000).contains(&retry(outcome)));
    let (tier, _, outcome) = step(&rig).await;
    assert_eq!((tier, retry(outcome)), (Tier::Direct, 0));
    let (tier, _, outcome) = step(&rig).await;
    assert_eq!(tier, Tier::Relay);
    assert!((1_000..=2_000).contains(&retry(outcome)));
    assert!(!rig.engine.status().online);
    let (tier, _, outcome) = step(&rig).await;
    assert_eq!((tier, outcome), (Tier::Direct, SyncOutcome::Idle));
    let relay = &rig.server.calls_to("POST", "/chat/sync")[1];
    assert_eq!(relay.tier, Tier::Relay);
    assert!(rig.engine.status().online);
    assert_eq!(rig.engine.status().tier, Some(Tier::Direct));
}

#[tokio::test]
async fn backoff_stops_growing_at_fifteen_seconds() {
    let rig = rig_on(&[Tier::Direct]);
    synced(&rig);
    for _ in 0..8 {
        rig.server
            .on("POST", "/chat/sync", Step::Fail(ChatError::Offline));
    }
    let mut waits = Vec::new();
    for _ in 0..8 {
        waits.push(retry(step(&rig).await.2));
    }
    assert!(waits.iter().all(|wait| *wait <= BACKOFF_MAX_MS));
    assert!(waits[7] >= BACKOFF_MAX_MS / 2);
}

#[tokio::test]
async fn mid_hold_drops_shorten_the_hold_then_move_to_the_next_tier() {
    let rig = rig_on(&[Tier::Direct, Tier::Relay]);
    synced(&rig);
    rig.engine.set_visible(true);
    for _ in 0..4 {
        rig.server.on(
            "POST",
            "/chat/sync",
            Step::FailAfter(5_000, ChatError::Offline),
        );
    }

    let mut seen = Vec::new();
    for _ in 0..4 {
        let (tier, hold, outcome) = step(&rig).await;
        assert!(retry(outcome) <= 10_000);
        seen.push((tier, hold));
    }
    let (tier, hold, _) = step(&rig).await;
    seen.push((tier, hold));

    assert_eq!(
        seen,
        vec![
            (Tier::Direct, 20_000),
            (Tier::Direct, 20_000),
            (Tier::Direct, 8_000),
            (Tier::Direct, 8_000),
            (Tier::Relay, 20_000),
        ]
    );
}

#[tokio::test]
async fn graceful_shutdown_waits_the_time_the_server_gives() {
    let rig = rig();
    synced(&rig);
    rig.server.on(
        "POST",
        "/chat/sync",
        Step::Empty(204, vec![("retry-after-ms".into(), "4200".into())]),
    );
    assert_eq!(step(&rig).await.2, SyncOutcome::Retry { after_ms: 4_200 });
}

#[tokio::test]
async fn a_revoked_device_stops_syncing() {
    let rig = rig();
    synced(&rig);
    rig.server.on(
        "POST",
        "/chat/sync",
        Step::Json(403, json!({"error": "chat_device_revoked"})),
    );
    let plan = rig.engine.next_plan().unwrap();
    assert_eq!(rig.engine.step(plan).await, Err(ChatError::DeviceRevoked));
}

#[tokio::test]
async fn busy_answers_wait_at_least_the_given_time_or_up_to_ten_seconds() {
    let rig = rig();
    synced(&rig);
    rig.server.on(
        "POST",
        "/chat/sync",
        Step::Fail(ChatError::Busy { retry_ms: 0 }),
    );
    rig.server.on(
        "POST",
        "/chat/sync",
        Step::Json(429, json!({"error": "chat_busy", "retry_ms": 2500})),
    );
    assert!(retry(step(&rig).await.2) <= 10_000);
    let spread = retry(step(&rig).await.2);
    assert!((2_500..=3_750).contains(&spread), "{spread}");
}

#[tokio::test]
async fn busy_retries_spread_so_clients_do_not_come_back_together() {
    let rig = rig();
    synced(&rig);
    for _ in 0..40 {
        rig.server.on(
            "POST",
            "/chat/sync",
            Step::Json(429, json!({"error": "chat_busy", "retry_ms": 1000})),
        );
    }
    let mut waits = std::collections::HashSet::new();
    for _ in 0..40 {
        waits.insert(retry(step(&rig).await.2));
    }
    assert!(waits.len() > 10, "{waits:?}");
    assert!(waits.iter().all(|wait| (1_000..=1_500).contains(wait)));
}

#[tokio::test]
async fn hidden_syncs_short_once_a_minute_and_showing_syncs_at_once() {
    let rig = rig();
    synced(&rig);
    rig.clock.block_sleeps_from(60_000);
    rig.server.json("GET", "/chat/devices", json!([]));
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));

    let shown = async {
        while rig.server.calls_to("POST", "/chat/sync").is_empty() {
            tokio::task::yield_now().await;
        }
        rig.engine.set_visible(true);
    };
    let (ran, ()) = tokio::join!(rig.engine.run(rig.clock.as_ref()), shown);
    ran.unwrap();

    let syncs = rig.server.calls_to("POST", "/chat/sync");
    assert_eq!(syncs[0].body["hold_ms"], json!(0));
    assert!(!syncs[0].long_poll);
    assert_eq!(syncs[1].body["hold_ms"], json!(20_000));
    assert!(syncs[1].long_poll);
    assert_eq!(syncs[1].timeout_ms, 35_000);
    assert_eq!(syncs[1].body["max_bytes"], json!(256 * 1024));
    assert!(rig.clock.slept().contains(&60_000));
}

#[tokio::test]
async fn stop_ends_the_wait_before_a_reset_at_once() {
    let rig = rig();
    rig.clock.block_sleeps_from(1);
    rig.server.json("GET", "/chat/devices", json!([]));
    let mut reset = sync_support::fixtures::answer(9, INC, vec![]);
    reset["reset"] = json!(true);
    rig.server.json("POST", "/chat/sync", reset);
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );
    let stopper = async {
        while rig.server.calls_to("POST", "/chat/sync").is_empty() {
            tokio::task::yield_now().await;
        }
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        rig.engine.stop();
    };
    let ran = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(rig.engine.run(rig.clock.as_ref()), stopper).0
    })
    .await;

    assert_eq!(ran, Ok(Ok(())));
    assert!(rig.server.calls_to("GET", "/chat/conversations").is_empty());
}

#[tokio::test]
async fn a_reset_waits_up_to_ten_seconds_before_paging_the_list() {
    let rig = rig();
    rig.server.json("GET", "/chat/devices", json!([]));
    let mut reset = sync_support::fixtures::answer(9, INC, vec![]);
    reset["reset"] = json!(true);
    rig.server.json("POST", "/chat/sync", reset);
    rig.server.json(
        "GET",
        "/chat/conversations",
        json!({"items": [], "next_cursor": null}),
    );

    rig.engine.run(rig.clock.as_ref()).await.unwrap();

    assert!(rig.clock.slept()[0] <= 10_000);
    assert_eq!(rig.server.calls_to("GET", "/chat/conversations").len(), 1);
    assert_eq!(
        rig.local.sync_state().unwrap(),
        SyncState {
            pos: 9,
            inc: Some(INC)
        }
    );
}

#[tokio::test]
async fn the_pro_tier_asks_for_small_answers_and_a_shorter_hold() {
    let rig = rig_on(&[Tier::Pro]);
    synced(&rig);
    rig.engine.set_visible(true);
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    let (tier, hold, _) = step(&rig).await;
    assert_eq!((tier, hold), (Tier::Pro, 15_000));
    let call = &rig.server.calls_to("POST", "/chat/sync")[0];
    assert_eq!(call.body["max_bytes"], json!(24 * 1024));
}

#[tokio::test]
async fn the_server_hold_cap_wins_over_the_tier_hold() {
    let rig = rig();
    rig.engine.set_visible(true);
    rig.engine.set_hold_cap(10_000);
    assert_eq!(rig.engine.plan(Tier::Direct).hold_ms, 10_000);
}

#[tokio::test]
async fn a_zero_hold_cap_polls_with_a_gap_instead_of_back_to_back() {
    let rig = rig();
    rig.engine.set_visible(true);
    rig.engine.set_hold_cap(0);
    let plan = rig.engine.plan(Tier::Direct);
    assert_eq!(
        (plan.hold_ms, plan.delay_ms),
        (0, sc_chat::sync::SHORT_POLL_GAP_MS)
    );
    rig.engine.set_hold_cap(10_000);
    assert_eq!(rig.engine.plan(Tier::Direct).delay_ms, 0);
}

#[tokio::test]
async fn an_early_empty_answer_to_a_parked_poll_waits_a_second() {
    let rig = rig();
    synced(&rig);
    rig.engine.set_visible(true);
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    rig.server
        .on("POST", "/chat/sync", Step::Empty(204, vec![]));
    assert_eq!(step(&rig).await.2, SyncOutcome::Retry { after_ms: 1_000 });
    rig.engine.set_visible(false);
    assert_eq!(step(&rig).await.2, SyncOutcome::Idle);
}
