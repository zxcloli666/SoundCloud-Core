mod store_support;

use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::store::Store;
use sc_chat::{ConversationKind, GoneMessage, MessageBody, MessageClass, Seq};
use store_support::{PEER, album, conv, envelope, open, post, row, store, summary};
use uuid::Uuid;

fn group(store: &Store) -> Uuid {
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    id
}

fn seqs(store: &Store, id: Uuid, before: Option<Seq>, limit: u32) -> (Vec<Seq>, bool) {
    let page = store.images_before(id, before, limit).unwrap();
    let seqs = page
        .messages
        .iter()
        .map(|message| message.seq.unwrap())
        .collect();
    (seqs, page.more)
}

#[test]
fn only_sent_image_posts_come_back_newest_first_in_pages() {
    let store = store();
    let id = group(&store);
    let other = group(&store);
    let mut rows = Vec::new();
    for seq in 1..=60 {
        rows.push(match seq % 10 {
            0 => album(id, seq, "кадр", &[seq as u128, 1_000 + seq as u128]),
            _ => post(id, seq, PEER, "текст"),
        });
    }
    rows.push(album(other, 1, "чужое", &[9_999]));
    store.put_messages(&rows).unwrap();

    assert_eq!(seqs(&store, id, None, 4), (vec![60, 50, 40, 30], true));
    assert_eq!(seqs(&store, id, Some(30), 4), (vec![20, 10], false));
    assert_eq!(seqs(&store, id, Some(10), 4), (Vec::new(), false));
    let page = store.images_before(id, None, 1).unwrap();
    let newest = &page.messages[0];
    assert_eq!(newest.attachments.len(), 2);
    assert_eq!(newest.attachments[0].media_id, Uuid::from_u128(60));
    assert_eq!(seqs(&store, other, None, 0), (vec![1], false));
}

#[test]
fn deleted_hidden_and_local_posts_stay_out_of_the_gallery() {
    let store = store();
    let id = group(&store);
    let delete = row(
        id,
        4,
        PEER,
        MessageClass::Edit,
        Some(2),
        open(&envelope(Body::Delete(proto::Delete { target: 2 }))),
    );
    let mut local = album(id, -1, "ещё не ушло", &[5]);
    local.local_only = true;
    store
        .put_messages(&[
            album(id, 1, "первое", &[1]),
            album(id, 2, "удалено", &[2]),
            album(id, 3, "скрыто", &[3]),
            delete,
            local,
        ])
        .unwrap();
    store
        .hide_messages(&[GoneMessage {
            conversation_id: id,
            seq: 3,
        }])
        .unwrap();

    let page = store.images_before(id, None, 10).unwrap();
    let shown: Vec<Seq> = page
        .messages
        .iter()
        .map(|message| message.seq.unwrap())
        .collect();
    assert_eq!(shown, vec![1]);
    assert!(
        page.messages
            .iter()
            .all(|message| message.body != MessageBody::Deleted)
    );
}
