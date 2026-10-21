mod store_support;

use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use sc_chat::crypto::Ck;
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::proto::{self, envelope::Body};
use sc_chat::search::Search;
use sc_chat::store::{COMPACTION_GAP_MS, Store, StoredContent};
use sc_chat::{
    ConversationKind, EmbedsQuery, GoneMessage, HistoryQuery, Member, MemberRole, MemberState,
    MessageBody, MessageClass, PeerMark, RichText, SearchQuery, SystemEvent, SystemKind, UserRef,
};
use store_support::{
    MemoryKeys, PEER, TempDir, conv, count, edit, envelope, open, post, post_with_track, react,
    row, store, summary,
};
use uuid::Uuid;

const PER_CONVERSATION: [&str; 8] = [
    "SELECT count(*) FROM messages WHERE conversation_id = ?1",
    "SELECT count(*) FROM message_embeds WHERE conversation_id = ?1",
    "SELECT count(*) FROM members WHERE conversation_id = ?1",
    "SELECT count(*) FROM marks WHERE conversation_id = ?1",
    "SELECT count(*) FROM conversation_keys WHERE conversation_id = ?1",
    "SELECT count(*) FROM drafts WHERE conversation_id = ?1",
    "SELECT count(*) FROM outbox WHERE conversation_id = ?1",
    "SELECT count(*) FROM conversations WHERE id = ?1",
];

fn hits(store: &Arc<Store>, text: &str) -> usize {
    Search::new(store.clone())
        .query(&SearchQuery {
            text: text.into(),
            conversation_id: None,
            limit: 50,
        })
        .unwrap()
        .len()
}

fn fill(store: &Arc<Store>, id: uuid::Uuid) {
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    store
        .put_members(
            id,
            &[Member {
                user: UserRef {
                    id: PEER.into(),
                    name: "Peer".into(),
                    avatar_url: None,
                    permalink: None,
                },
                role: MemberRole::Owner,
                state: MemberState::Active,
                accepted: true,
                joined_seq: 0,
                invited_by: None,
            }],
        )
        .unwrap();
    store
        .put_marks(
            id,
            &[PeerMark {
                user_id: PEER.into(),
                read_seq: 2,
                delivered_seq: 3,
            }],
        )
        .unwrap();
    let system = SystemEvent {
        kind: SystemKind::Joined,
        actor: Some(PEER.into()),
        target: None,
        name: None,
        ttl_s: None,
        until: None,
        track: None,
        role: None,
        on: None,
    };
    store
        .put_messages(&[
            post(id, 1, PEER, "секретный план"),
            post_with_track(id, 2, "трек", "Nightcall"),
            edit(id, 3, PEER, 1, "секретный план изменён"),
            react(id, 4, PEER, 2, "🔥", true),
            row(
                id,
                5,
                PEER,
                MessageClass::Post,
                None,
                StoredContent::Sealed(vec![7; 64]),
            ),
            row(
                id,
                6,
                "",
                MessageClass::System,
                None,
                StoredContent::System(system),
            ),
        ])
        .unwrap();
    store.put_ck(id, 0, &Ck::from_bytes([3; 32])).unwrap();
    store
        .save_draft(
            id,
            Some(&RichText {
                text: "черновик".into(),
                spans: vec![],
            }),
        )
        .unwrap();
    Outbox::new(store.clone())
        .push(OutboxItem {
            client_id: Uuid::new_v4(),
            conversation_id: id,
            class: MessageClass::Post,
            target_seq: None,
            envelope: vec![1, 2, 3],
            created_at: 10,
            attempts: 0,
            next_at: 10,
        })
        .unwrap();
}

#[test]
fn purge_leaves_no_row_fts_entry_or_cached_key() {
    let store = Arc::new(store());
    let id = conv();
    let kept = conv();
    fill(&store, id);
    fill(&store, kept);
    for sql in PER_CONVERSATION {
        assert!(count(&store, sql, id) > 0, "{sql}");
    }
    assert_eq!(hits(&store, "секрет"), 2);
    assert_eq!(hits(&store, "nightcall"), 2);

    let only_there = UserRef {
        id: "777".into(),
        name: "Только тут".into(),
        avatar_url: None,
        permalink: None,
    };
    let mut direct = summary(id, ConversationKind::Direct);
    direct.peer = Some(only_there.clone());
    store.put_conversations(&[direct]).unwrap();
    assert_eq!(store.users(&["777".into()]).unwrap(), vec![only_there]);

    store.purge_conversations(&[id]).unwrap();

    for sql in PER_CONVERSATION {
        assert_eq!(count(&store, sql, id), 0, "{sql}");
        assert!(count(&store, sql, kept) > 0, "{sql}");
    }
    assert!(store.users(&["777".into()]).unwrap().is_empty());
    assert_eq!(store.users(&[PEER.into()]).unwrap().len(), 1);
    assert_eq!(hits(&store, "секрет"), 1);
    assert_eq!(hits(&store, "nightcall"), 1);
    assert!(store.ck(id, 0).unwrap().is_none());
    assert!(store.draft(id).unwrap().is_none());
    assert!(store.conversation(id).unwrap().is_none());
    assert_eq!(store.ck(kept, 0).unwrap().unwrap().as_bytes(), &[3; 32]);
}

#[test]
fn gone_messages_drop_their_index_embeds_and_modifiers() {
    let store = Arc::new(store());
    let id = conv();
    fill(&store, id);
    store
        .purge_messages(&[
            GoneMessage {
                conversation_id: id,
                seq: 1,
            },
            GoneMessage {
                conversation_id: id,
                seq: 2,
            },
        ])
        .unwrap();
    assert_eq!(hits(&store, "секрет"), 0);
    assert_eq!(hits(&store, "nightcall"), 0);
    let embeds = store
        .embeds(
            id,
            &EmbedsQuery {
                kinds: vec![],
                before: None,
                limit: 10,
            },
        )
        .unwrap();
    assert!(embeds.items.is_empty());
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM messages WHERE conversation_id = ?1 AND seq IN (1, 2, 3, 4)",
            id
        ),
        0
    );
    let page = store
        .messages(
            id,
            &HistoryQuery {
                before: None,
                after: None,
                limit: 50,
            },
        )
        .unwrap();
    let seqs: Vec<_> = page.messages.iter().map(|message| message.seq).collect();
    assert_eq!(seqs, vec![Some(5), Some(6)]);
}

#[test]
fn a_message_deleted_for_me_stays_gone_when_history_brings_it_back() {
    let store = Arc::new(store());
    let id = conv();
    store
        .put_messages(&[post(id, 1, PEER, "секрет"), post(id, 2, PEER, "видно")])
        .unwrap();
    let hidden = GoneMessage {
        conversation_id: id,
        seq: 1,
    };
    store.hide_messages(&[hidden]).unwrap();
    store
        .put_messages(&[
            post(id, 1, PEER, "секрет"),
            edit(id, 3, PEER, 1, "секрет поправлен"),
            react(id, 4, PEER, 1, "🔥", true),
        ])
        .unwrap();
    assert_eq!(hits(&store, "секрет"), 0);
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM messages WHERE conversation_id = ?1 AND seq IN (1, 3, 4)",
            id
        ),
        0
    );
    let page = store
        .messages(
            id,
            &HistoryQuery {
                before: None,
                after: None,
                limit: 50,
            },
        )
        .unwrap();
    let seqs: Vec<_> = page.messages.iter().map(|message| message.seq).collect();
    assert_eq!(seqs, vec![Some(2)]);
    store.purge_conversations(&[id]).unwrap();
    assert_eq!(
        count(
            &store,
            "SELECT count(*) FROM hidden_messages WHERE conversation_id = ?1",
            id
        ),
        0
    );
}

#[test]
fn wipe_empties_the_store_and_keeps_it_usable() {
    let store = Arc::new(store());
    let id = conv();
    fill(&store, id);
    store.wipe().unwrap();
    for sql in PER_CONVERSATION {
        assert_eq!(count(&store, sql, id), 0, "{sql}");
    }
    assert_eq!(hits(&store, "секрет"), 0);
    assert!(store.me().unwrap().is_none());
    store
        .put_messages(&[post(id, 1, PEER, "снова тут")])
        .unwrap();
    assert_eq!(hits(&store, "снова"), 1);
}

#[test]
fn purge_scrubs_the_database_and_wal_files() {
    let dir = TempDir::new("purge");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let store = Arc::new(Store::open_keyed(&path, &keys).unwrap());
    let id = conv();
    let marker = "уникальная-фраза ".repeat(40);
    store.put_messages(&[post(id, 1, PEER, &marker)]).unwrap();
    store.put_ck(id, 0, &Ck::from_bytes([5; 32])).unwrap();
    let (body, ck): (Vec<u8>, Vec<u8>) = {
        let conn = store.conn().unwrap();
        let body = conn
            .query_row("SELECT body FROM messages", [], |row| row.get(0))
            .unwrap();
        let ck = conn
            .query_row("SELECT ck FROM conversation_keys", [], |row| row.get(0))
            .unwrap();
        (body, ck)
    };
    let probes = [&body[body.len() / 2..body.len() / 2 + 32], &ck[..32]];
    for probe in probes {
        assert!(on_disk(&path, probe));
    }
    let tokens = tokens(&store.keys().search, "уникальная");
    assert!(
        tokens
            .iter()
            .any(|token| on_disk(&path, &token.as_bytes()[4..]))
    );

    store.purge_conversations(&[id]).unwrap();

    drop(store);
    for probe in probes {
        assert!(!on_disk(&path, probe));
    }
    for token in &tokens {
        assert!(!on_disk(&path, &token.as_bytes()[4..]));
    }
}

#[test]
fn deleted_bodies_and_their_edits_leave_the_disk() {
    let dir = TempDir::new("deleted");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let store = Arc::new(Store::open_keyed(&path, &keys).unwrap());
    let id = conv();
    store
        .put_messages(&[
            post(id, 1, PEER, &"сервер-стёр ".repeat(30)),
            post(id, 2, PEER, &"сам-удалил ".repeat(30)),
            edit(id, 3, PEER, 2, &"правка-ушла ".repeat(30)),
        ])
        .unwrap();
    let probes: Vec<Vec<u8>> = [1, 2, 3]
        .iter()
        .map(|seq| body_probe(&store, *seq))
        .collect();
    for probe in &probes {
        assert!(on_disk(&path, probe));
    }

    let redacted = row(
        id,
        1,
        PEER,
        MessageClass::Post,
        None,
        StoredContent::Deleted,
    );
    let delete = row(
        id,
        4,
        PEER,
        MessageClass::Edit,
        Some(2),
        open(&envelope(Body::Delete(proto::Delete { target: 2 }))),
    );
    store.put_messages(&[redacted, delete]).unwrap();

    for probe in &probes {
        assert!(!on_disk(&path, probe));
    }
    for word in ["сервер", "сам", "правка"] {
        for token in tokens(&store.keys().search, word) {
            assert!(!on_disk(&path, &token.as_bytes()[4..]));
        }
    }
    let page = store
        .messages(
            id,
            &HistoryQuery {
                before: None,
                after: None,
                limit: 10,
            },
        )
        .unwrap();
    let bodies: Vec<_> = page.messages.iter().map(|message| &message.body).collect();
    assert_eq!(bodies, vec![&MessageBody::Deleted, &MessageBody::Deleted]);
}

#[test]
fn expiry_scrubs_the_body_at_once_and_the_index_within_a_minute() {
    let dir = TempDir::new("expiry");
    let path = dir.file("chat.db");
    let keys = MemoryKeys::default();
    let store = Arc::new(Store::open_keyed(&path, &keys).unwrap());
    let id = conv();
    let mut first = post(id, 1, PEER, &"первая-тайна ".repeat(30));
    first.expires_at = Some(1_000);
    let mut second = post(id, 2, PEER, &"вторая-тайна ".repeat(30));
    second.expires_at = Some(20_000);
    store.put_messages(&[first, second]).unwrap();
    let first_body = body_probe(&store, 1);
    let second_body = body_probe(&store, 2);
    let first_tokens = tokens(&store.keys().search, "первая");
    let second_tokens = tokens(&store.keys().search, "вторая");
    let indexed = |tokens: &[String]| {
        tokens
            .iter()
            .any(|token| on_disk(&path, &token.as_bytes()[4..]))
    };
    assert!(indexed(&first_tokens) && indexed(&second_tokens));

    assert_eq!(store.purge_expired(1_000).unwrap().len(), 1);
    assert!(!on_disk(&path, &first_body));
    assert!(!indexed(&first_tokens));

    assert_eq!(store.purge_expired(20_000).unwrap().len(), 1);
    assert!(!on_disk(&path, &second_body));
    assert!(indexed(&second_tokens));
    assert!(
        store
            .purge_expired(1_000 + COMPACTION_GAP_MS - 1)
            .unwrap()
            .is_empty()
    );
    assert!(indexed(&second_tokens));

    assert!(
        store
            .purge_expired(1_000 + COMPACTION_GAP_MS)
            .unwrap()
            .is_empty()
    );
    assert!(!indexed(&second_tokens));
    assert_eq!(hits(&store, "вторая"), 0);
}

fn body_probe(store: &Store, seq: i64) -> Vec<u8> {
    let body: Vec<u8> = store
        .conn()
        .unwrap()
        .query_row("SELECT body FROM messages WHERE seq = ?1", [seq], |row| {
            row.get(0)
        })
        .unwrap();
    body[body.len() / 2..body.len() / 2 + 32].to_vec()
}

fn tokens(key: &[u8; 32], word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    (2..=chars.len())
        .map(|len| {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
            mac.update(chars[..len].iter().collect::<String>().as_bytes());
            mac.finalize().into_bytes()[..12]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        })
        .collect()
}

fn on_disk(path: &std::path::Path, probe: &[u8]) -> bool {
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    [path.to_path_buf(), wal.into()].iter().any(|file| {
        std::fs::read(file)
            .map(|bytes| bytes.windows(probe.len()).any(|window| window == probe))
            .unwrap_or(false)
    })
}
