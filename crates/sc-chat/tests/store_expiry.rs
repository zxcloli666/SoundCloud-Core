mod store_support;

use std::sync::Arc;

use sc_chat::search::Search;
use sc_chat::store::expiry::NAP_MAX_MS;
use sc_chat::store::{ExpiryTimer, Store};
use sc_chat::{CoreEvent, EmbedsQuery, GoneMessage, HistoryQuery, SearchQuery};
use store_support::{ManualClock, PEER, Recorded, conv, post, post_with_track, react, store};

struct Rig {
    store: Arc<Store>,
    clock: Arc<ManualClock>,
    events: Arc<Recorded>,
    timer: ExpiryTimer,
}

fn rig() -> Rig {
    let store = Arc::new(store());
    let clock = Arc::new(ManualClock::default());
    let events = Arc::new(Recorded::default());
    let timer = ExpiryTimer::new(store.clone(), clock.clone(), events.clone());
    Rig {
        store,
        clock,
        events,
        timer,
    }
}

fn visible(store: &Store, id: uuid::Uuid) -> Vec<i64> {
    store
        .messages(
            id,
            &HistoryQuery {
                before: None,
                after: None,
                limit: 50,
            },
        )
        .unwrap()
        .messages
        .iter()
        .filter_map(|message| message.seq)
        .collect()
}

fn hits(store: &Arc<Store>, text: &str) -> usize {
    Search::new(store.clone())
        .query(&SearchQuery {
            text: text.into(),
            conversation_id: None,
            limit: 10,
        })
        .unwrap()
        .len()
}

fn embeds(store: &Store, id: uuid::Uuid) -> usize {
    store
        .embeds(
            id,
            &EmbedsQuery {
                kinds: vec![],
                before: None,
                limit: 10,
            },
        )
        .unwrap()
        .items
        .len()
}

#[test]
fn an_expired_message_goes_with_its_index_and_embeds_while_offline() {
    let rig = rig();
    let id = conv();
    let mut timed = post_with_track(id, 1, "исчезающее", "Strobe");
    timed.expires_at = Some(5_000);
    let mut reaction = react(id, 2, PEER, 1, "👍", true);
    reaction.expires_at = Some(5_000);
    rig.store
        .put_messages(&[timed, reaction, post(id, 3, PEER, "останусь")])
        .unwrap();
    assert_eq!(rig.store.next_expiry().unwrap(), Some(5_000));

    rig.clock.set(4_000);
    assert_eq!(rig.timer.sweep().unwrap(), 1_000);
    assert_eq!(visible(&rig.store, id), vec![1, 3]);
    assert!(rig.events.0.lock().unwrap().is_empty());

    rig.clock.set(5_000);
    assert_eq!(rig.timer.sweep().unwrap(), NAP_MAX_MS);
    assert_eq!(visible(&rig.store, id), vec![3]);
    assert_eq!(hits(&rig.store, "исчез"), 0);
    assert_eq!(hits(&rig.store, "strobe"), 0);
    assert_eq!(hits(&rig.store, "остан"), 1);
    assert_eq!(embeds(&rig.store, id), 0);
    assert_eq!(rig.store.next_expiry().unwrap(), None);

    let events = rig.events.0.lock().unwrap();
    let [CoreEvent::Gone(gone)] = events.as_slice() else {
        panic!("expected one gone event, got {events:?}");
    };
    assert!(gone.conversations.is_empty());
    assert!(gone.messages.contains(&GoneMessage {
        conversation_id: id,
        seq: 1
    }));
}

#[test]
fn the_nap_never_exceeds_the_cap() {
    let rig = rig();
    let id = conv();
    let mut later = post(id, 1, PEER, "через час");
    later.expires_at = Some(3_600_000);
    rig.store.put_messages(&[later]).unwrap();
    rig.clock.set(0);
    assert_eq!(rig.timer.sweep().unwrap(), NAP_MAX_MS);
}

#[tokio::test]
async fn the_timer_loop_catches_up_on_its_own() {
    let rig = Arc::new(rig());
    let id = conv();
    let mut soon = post(id, 1, PEER, "скоро");
    soon.expires_at = Some(40_000);
    rig.store.put_messages(&[soon]).unwrap();
    let looping = rig.clone();
    let handle = tokio::spawn(async move { looping.timer.run().await });
    for _ in 0..100 {
        if visible(&rig.store, id).is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    handle.abort();
    assert!(visible(&rig.store, id).is_empty());
    assert!(rig.clock.0.load(std::sync::atomic::Ordering::Relaxed) >= 40_000);
}

#[test]
fn gone_msgs_from_sync_purge_the_same_way() {
    let rig = rig();
    let id = conv();
    let mut timed = post_with_track(id, 1, "уйду", "Genesis");
    timed.expires_at = Some(99_000);
    rig.store.put_messages(&[timed]).unwrap();
    rig.store
        .purge_messages(&[GoneMessage {
            conversation_id: id,
            seq: 1,
        }])
        .unwrap();
    assert!(visible(&rig.store, id).is_empty());
    assert_eq!(hits(&rig.store, "уйду"), 0);
    assert_eq!(embeds(&rig.store, id), 0);
    assert_eq!(rig.store.next_expiry().unwrap(), None);
}
