mod store_support;

use std::sync::Arc;

use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::msg::encode_padded;
use sc_chat::sc_chat_wire::proto::envelope::Body;
use sc_chat::store::Store;
use sc_chat::{
    ConversationKind, DeliveryState, HistoryQuery, MessageBody, MessageClass, SearchQuery,
};
use store_support::{ME, MemoryKeys, PEER, TempDir, conv, envelope, post, store, summary, text};
use uuid::Uuid;

fn item(conversation_id: uuid::Uuid, body: &str, at: i64) -> OutboxItem {
    OutboxItem {
        client_id: Uuid::new_v4(),
        conversation_id,
        class: MessageClass::Post,
        target_seq: None,
        envelope: encode_padded(&envelope(Body::Text(text(body)))),
        created_at: at,
        attempts: 0,
        next_at: at,
    }
}

fn history(store: &Store, id: uuid::Uuid) -> Vec<sc_chat::Message> {
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
}

#[test]
fn the_outbox_survives_a_restart() {
    let dir = TempDir::new("outbox");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let id = conv();
    let queued = item(id, "переживу перезапуск", 1_000);
    {
        let store = Arc::new(Store::open_keyed(&path, &keys).unwrap());
        store.set_me(ME).unwrap();
        let pending = Outbox::new(store).push(queued.clone()).unwrap();
        assert_eq!(pending.state, DeliveryState::Pending);
        assert!(pending.mine);
    }
    let store = Arc::new(Store::open_keyed(&path, &keys).unwrap());
    let outbox = Outbox::new(store);
    assert_eq!(outbox.due(1_000, 10).unwrap(), vec![queued.clone()]);
    assert!(outbox.due(999, 10).unwrap().is_empty());
    let pending = outbox.pending(id).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].client_id, queued.client_id);
    assert_eq!(pending[0].sender, ME);
    let MessageBody::Text(body) = &pending[0].body else {
        panic!("pending body is not text");
    };
    assert_eq!(body.text, "переживу перезапуск");
}

#[test]
fn due_items_keep_their_order() {
    let outbox = Outbox::new(Arc::new(store()));
    let id = conv();
    let first = item(id, "раз", 10);
    let second = item(id, "два", 20);
    outbox.push(second.clone()).unwrap();
    outbox.push(first.clone()).unwrap();
    let due: Vec<_> = outbox
        .due(30, 10)
        .unwrap()
        .into_iter()
        .map(|item| item.client_id)
        .collect();
    assert_eq!(due, vec![first.client_id, second.client_id]);
    assert_eq!(outbox.due(30, 1).unwrap().len(), 1);
}

#[test]
fn a_failed_send_waits_for_a_retry() {
    let outbox = Outbox::new(Arc::new(store()));
    let id = conv();
    let queued = item(id, "не ушло", 10);
    outbox.push(queued.clone()).unwrap();

    outbox.failed(queued.client_id, Some(500)).unwrap();
    assert!(outbox.due(499, 10).unwrap().is_empty());
    let due = outbox.due(500, 10).unwrap();
    assert_eq!(due[0].attempts, 1);

    outbox.failed(queued.client_id, None).unwrap();
    assert!(outbox.due(10_000, 10).unwrap().is_empty());
    assert_eq!(outbox.pending(id).unwrap()[0].state, DeliveryState::Failed);

    outbox.retry(queued.client_id, 2_000).unwrap();
    assert_eq!(outbox.due(2_000, 10).unwrap()[0].attempts, 2);
    assert_eq!(outbox.pending(id).unwrap()[0].state, DeliveryState::Pending);

    outbox.discard(queued.client_id).unwrap();
    assert!(outbox.pending(id).unwrap().is_empty());
}

#[test]
fn a_sent_item_moves_into_history() {
    let store = Arc::new(store());
    let outbox = Outbox::new(store.clone());
    let id = conv();
    let queued = item(id, "доставлено", 10);
    outbox.push(queued.clone()).unwrap();
    outbox.sent(queued.client_id, 7, 5_000, None).unwrap();

    assert!(outbox.due(i64::MAX, 10).unwrap().is_empty());
    assert!(outbox.pending(id).unwrap().is_empty());
    let messages = history(&store, id);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].seq, Some(7));
    assert_eq!(messages[0].client_id, queued.client_id);
    assert!(messages[0].mine);
    assert_eq!(messages[0].expires_at, None);
    let found = sc_chat::search::Search::new(store)
        .query(&SearchQuery {
            text: "достав".into(),
            conversation_id: Some(id),
            limit: 5,
        })
        .unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn a_sent_item_takes_the_server_timer_even_when_the_local_chat_has_none() {
    let store = Arc::new(store());
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Direct)])
        .unwrap();
    let outbox = Outbox::new(store.clone());
    let queued = item(id, "исчезну", 10);
    outbox.push(queued.clone()).unwrap();
    outbox
        .sent(queued.client_id, 1, 5_000, Some(5_000 + 3_600_000))
        .unwrap();
    assert_eq!(history(&store, id)[0].expires_at, Some(5_000 + 3_600_000));
    assert_eq!(store.next_expiry().unwrap(), Some(5_000 + 3_600_000));
}

#[test]
fn the_synced_copy_settles_a_pending_item() {
    let store = Arc::new(store());
    let outbox = Outbox::new(store.clone());
    let id = conv();
    let queued = item(id, "ответ потерялся", 10);
    outbox.push(queued.clone()).unwrap();
    let mut synced = post(id, 3, ME, "ответ потерялся");
    synced.client_id = queued.client_id;
    store.put_messages(&[synced]).unwrap();
    assert!(outbox.due(i64::MAX, 10).unwrap().is_empty());
    outbox.sent(queued.client_id, 3, 3_000, None).unwrap();
    assert_eq!(history(&store, id).len(), 1);
}

#[test]
fn a_stranger_cannot_settle_my_item_from_another_chat() {
    let store = Arc::new(store());
    let outbox = Outbox::new(store.clone());
    let queued = item(conv(), "моё", 10);
    outbox.push(queued.clone()).unwrap();
    let mut forged = post(conv(), 1, PEER, "чужое");
    forged.client_id = queued.client_id;
    store.put_messages(&[forged]).unwrap();
    assert_eq!(outbox.due(i64::MAX, 10).unwrap().len(), 1);
}

#[test]
fn system_rows_and_local_targets_never_queue() {
    let outbox = Outbox::new(Arc::new(store()));
    let mut system = item(conv(), "x", 1);
    system.class = MessageClass::System;
    assert!(outbox.push(system).is_err());
    let mut on_local = item(conv(), "x", 1);
    on_local.class = MessageClass::React;
    on_local.target_seq = Some(-5);
    assert!(outbox.push(on_local).is_err());
}
