mod store_support;

use std::sync::Arc;

use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::msg::encode_padded;
use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::search::Search;
use sc_chat::store::Store;
use sc_chat::{ConversationKind, HistoryQuery, Message, MessageBody, MessageClass, SearchQuery};
use store_support::{PEER, conv, edit, envelope, post, react, store, summary};
use uuid::Uuid;

fn page(store: &Store, id: uuid::Uuid, query: HistoryQuery) -> Vec<Message> {
    store.messages(id, &query).unwrap().messages
}

fn latest(store: &Store, id: uuid::Uuid) -> Vec<Message> {
    page(
        store,
        id,
        HistoryQuery {
            before: None,
            after: None,
            limit: 50,
        },
    )
}

fn texts(messages: &[Message]) -> Vec<(String, bool)> {
    messages
        .iter()
        .map(|message| match &message.body {
            MessageBody::Text(text) => (text.text.clone(), message.local_only),
            other => (format!("{other:?}"), message.local_only),
        })
        .collect()
}

fn restored() -> (Arc<Store>, uuid::Uuid) {
    let store = Arc::new(store());
    let id = conv();
    let mut conversation = summary(id, ConversationKind::Direct);
    conversation.last_seq = 6;
    store.put_conversations(&[conversation]).unwrap();
    store
        .put_messages(&[
            post(id, 1, PEER, "один"),
            post(id, 2, PEER, "два"),
            post(id, 3, PEER, "три"),
            post(id, 4, PEER, "четыре"),
            edit(id, 5, PEER, 4, "четыре правка"),
            react(id, 6, PEER, 2, "❤", true),
        ])
        .unwrap();
    store.keep_local_above(&[(id, 3)]).unwrap();
    (store, id)
}

#[test]
fn rows_above_the_restored_seq_stay_on_this_device() {
    let (store, id) = restored();
    assert_eq!(
        texts(&latest(&store, id)),
        vec![
            ("один".into(), false),
            ("два".into(), false),
            ("три".into(), false),
            ("четыре правка".into(), true),
        ]
    );
    let messages = latest(&store, id);
    assert!(messages[3].seq.unwrap() < 0);
    assert!(messages[3].edited_at.is_some());
    assert!(messages[1].reactions.is_empty());
    assert_eq!(store.known(10).unwrap(), vec![(id, 3)]);
}

#[test]
fn new_server_rows_take_the_freed_seqs_after_the_block() {
    let (store, id) = restored();
    let mut fresh = post(id, 4, PEER, "новое после восстановления");
    fresh.ts = 60_000;
    store.put_messages(&[fresh]).unwrap();
    assert_eq!(
        texts(&latest(&store, id)),
        vec![
            ("один".into(), false),
            ("два".into(), false),
            ("три".into(), false),
            ("четыре правка".into(), true),
            ("новое после восстановления".into(), false),
        ]
    );
    let older = page(
        &store,
        id,
        HistoryQuery {
            before: Some(3),
            after: None,
            limit: 1,
        },
    );
    assert_eq!(texts(&older), vec![("два".into(), false)]);
    let newer = page(
        &store,
        id,
        HistoryQuery {
            before: None,
            after: Some(3),
            limit: 10,
        },
    );
    assert_eq!(
        texts(&newer),
        vec![("новое после восстановления".into(), false)]
    );
    let around = page(
        &store,
        id,
        HistoryQuery {
            before: Some(4),
            after: None,
            limit: 1,
        },
    );
    assert_eq!(
        texts(&around),
        vec![("три".into(), false), ("четыре правка".into(), true)]
    );
}

#[test]
fn local_rows_stay_searchable() {
    let (store, _) = restored();
    let found = Search::new(store)
        .query(&SearchQuery {
            text: "правка".into(),
            conversation_id: None,
            limit: 5,
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].seq < 0);
}

#[test]
fn a_second_restore_keeps_both_blocks_apart() {
    let (store, id) = restored();
    let mut fresh = post(id, 4, PEER, "вторая жизнь");
    fresh.ts = 60_000;
    store.put_messages(&[fresh]).unwrap();
    store.keep_local_above(&[(id, 3)]).unwrap();
    let messages = latest(&store, id);
    assert_eq!(messages.len(), 5);
    assert_eq!(
        messages.iter().filter(|message| message.local_only).count(),
        2
    );
    let mut seqs: Vec<_> = messages.iter().filter_map(|message| message.seq).collect();
    seqs.dedup();
    assert_eq!(seqs.len(), 5);
}

#[test]
fn a_server_row_can_not_take_a_local_seq() {
    let (store, id) = restored();
    let local = latest(&store, id)[3].seq.unwrap();
    store
        .put_messages(&[post(id, local, PEER, "подмена"), post(id, 0, PEER, "ноль")])
        .unwrap();
    assert_eq!(
        texts(&latest(&store, id)),
        vec![
            ("один".into(), false),
            ("два".into(), false),
            ("три".into(), false),
            ("четыре правка".into(), true),
        ]
    );
}

#[test]
fn queued_actions_on_moved_rows_are_dropped() {
    let store = Arc::new(store());
    let id = conv();
    store
        .put_messages(&[post(id, 1, PEER, "один"), post(id, 2, PEER, "два")])
        .unwrap();
    let outbox = Outbox::new(store.clone());
    let reaction = |target: i64| OutboxItem {
        client_id: Uuid::new_v4(),
        conversation_id: id,
        class: MessageClass::React,
        target_seq: Some(target),
        envelope: encode_padded(&envelope(Body::Reaction(proto::Reaction {
            target: target as u64,
            emoji: "👍".into(),
            on: true,
        }))),
        created_at: 10,
        attempts: 0,
        next_at: 10,
    };
    let kept = reaction(1);
    outbox.push(kept.clone()).unwrap();
    outbox.push(reaction(2)).unwrap();
    store.keep_local_above(&[(id, 1)]).unwrap();
    assert_eq!(outbox.due(10, 10).unwrap(), vec![kept]);
}
