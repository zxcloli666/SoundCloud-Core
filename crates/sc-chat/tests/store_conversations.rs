mod store_support;

use sc_chat::crypto::Ck;
use sc_chat::{
    ConversationKind, EmbedKind, EmbedsQuery, GoneMessage, ListFilter, ListQuery, Member,
    MemberRole, MemberState, MessagePreview, PreviewKind, RichText, UserRef,
};
use store_support::{PEER, conv, post, post_with_track, store, summary};

fn ids(store: &sc_chat::store::Store, filter: ListFilter) -> Vec<uuid::Uuid> {
    store
        .conversations(&ListQuery {
            filter,
            cursor: None,
            limit: 50,
        })
        .unwrap()
        .items
        .into_iter()
        .map(|item| item.id)
        .collect()
}

fn peer() -> UserRef {
    UserRef {
        id: PEER.into(),
        name: "Пир".into(),
        avatar_url: Some("https://i1.sndcdn.com/avatars-x.jpg".into()),
        permalink: Some("peer".into()),
    }
}

#[test]
fn the_list_filters_and_orders_conversations() {
    let store = store();
    let direct = conv();
    let group = conv();
    let request = conv();
    let archived = conv();
    let pinned = conv();
    let mut rows = vec![
        summary(direct, ConversationKind::Direct),
        summary(group, ConversationKind::Group),
        summary(request, ConversationKind::Direct),
        summary(archived, ConversationKind::Group),
        summary(pinned, ConversationKind::Group),
    ];
    rows[0].last_at = 50;
    rows[0].unread = 2;
    rows[1].last_at = 40;
    rows[2].accepted = false;
    rows[3].archived = true;
    rows[4].pinned_at = Some(1);
    store.put_conversations(&rows).unwrap();
    assert_eq!(ids(&store, ListFilter::All), vec![pinned, direct, group]);
    assert_eq!(ids(&store, ListFilter::Direct), vec![direct]);
    assert_eq!(ids(&store, ListFilter::Groups), vec![pinned, group]);
    assert_eq!(ids(&store, ListFilter::Unread), vec![direct]);
    assert_eq!(ids(&store, ListFilter::Requests), vec![request]);
    assert_eq!(ids(&store, ListFilter::Archived), vec![archived]);

    let first = store
        .conversations(&ListQuery {
            filter: ListFilter::All,
            cursor: None,
            limit: 2,
        })
        .unwrap();
    assert_eq!(first.items.len(), 2);
    let rest = store
        .conversations(&ListQuery {
            filter: ListFilter::All,
            cursor: first.next_cursor.clone(),
            limit: 2,
        })
        .unwrap();
    assert_eq!(
        rest.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![group]
    );
    assert_eq!(rest.next_cursor, None);
}

#[test]
fn a_summary_round_trips_with_peer_draft_and_preview() {
    let store = store();
    let id = conv();
    let mut row = summary(id, ConversationKind::Direct);
    row.peer = Some(peer());
    row.title = Some("Секретное имя".into());
    row.ttl_s = Some(86_400);
    row.muted_until = Some(9_000);
    store.put_conversations(&[row.clone()]).unwrap();
    let saved = store.conversation(id).unwrap().unwrap();
    assert_eq!(saved, row);

    store
        .save_draft(
            id,
            Some(&RichText {
                text: "черновик".into(),
                spans: vec![],
            }),
        )
        .unwrap();
    store
        .put_messages(&[
            post(id, 1, PEER, "первое"),
            post(id, 2, PEER, "последнее слово"),
        ])
        .unwrap();
    let saved = store.conversation(id).unwrap().unwrap();
    assert!(saved.has_draft);
    let preview = saved.preview.unwrap();
    assert_eq!(preview.kind, PreviewKind::Text);
    assert_eq!(preview.text, "последнее слово");
    assert_eq!(preview.seq, 2);
    assert_eq!(store.draft(id).unwrap().unwrap().text, "черновик");

    store.save_draft(id, Some(&RichText::default())).unwrap();
    assert!(store.draft(id).unwrap().is_none());
    assert!(!store.conversation(id).unwrap().unwrap().has_draft);
    assert_eq!(
        store.users(&[PEER.into(), "404".into()]).unwrap(),
        vec![peer()]
    );
}

#[test]
fn an_embed_only_post_previews_its_card() {
    let store = store();
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    store
        .put_messages(&[post_with_track(id, 1, "", "Kids")])
        .unwrap();
    let preview = store.conversation(id).unwrap().unwrap().preview.unwrap();
    assert_eq!(preview.kind, PreviewKind::Embed);
    assert_eq!(preview.text, "Kids");
}

#[test]
fn embeds_page_by_kind_and_seq() {
    let store = store();
    let id = conv();
    store
        .put_messages(&[
            post_with_track(id, 1, "раз", "One"),
            post_with_track(id, 2, "два", "Two"),
            post_with_track(id, 3, "три", "Three"),
        ])
        .unwrap();
    let query = |before, limit| EmbedsQuery {
        kinds: vec![EmbedKind::Track],
        before,
        limit,
    };
    let first = store.embeds(id, &query(None, 2)).unwrap();
    let titles: Vec<_> = first
        .items
        .iter()
        .map(|hit| hit.embed.title.as_str())
        .collect();
    assert_eq!(titles, vec!["Three", "Two"]);
    assert!(first.more);
    let rest = store.embeds(id, &query(Some(2), 2)).unwrap();
    assert_eq!(rest.items[0].embed.title, "One");
    assert_eq!(rest.items[0].sender, PEER);
    assert!(!rest.more);
    let playlists = EmbedsQuery {
        kinds: vec![EmbedKind::Playlist],
        before: None,
        limit: 5,
    };
    assert!(store.embeds(id, &playlists).unwrap().items.is_empty());
}

#[test]
fn members_marks_keys_and_epochs_are_kept() {
    let store = store();
    let id = conv();
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    let owner = Member {
        user: peer(),
        role: MemberRole::Owner,
        state: MemberState::Active,
        accepted: true,
        joined_seq: 0,
        invited_by: None,
    };
    store.put_members(id, std::slice::from_ref(&owner)).unwrap();
    let members = store.members(id).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].user, peer());
    assert_eq!(members[0].role, MemberRole::Owner);
    store.put_members(id, &[]).unwrap();
    assert!(store.members(id).unwrap().is_empty());

    assert!(store.ck(id, 1).unwrap().is_none());
    store.put_ck(id, 1, &Ck::from_bytes([4; 32])).unwrap();
    assert_eq!(store.ck(id, 1).unwrap().unwrap().as_bytes(), &[4; 32]);
    assert!(store.ck(id, 2).unwrap().is_none());

    assert_eq!(store.key_epoch(id).unwrap(), Some(0));
    store.set_key_epoch(id, 5).unwrap();
    assert_eq!(store.key_epoch(id).unwrap(), Some(5));
    store
        .put_conversations(&[summary(id, ConversationKind::Group)])
        .unwrap();
    assert_eq!(store.key_epoch(id).unwrap(), Some(5));
}

#[test]
fn known_lists_recent_conversations_first() {
    let store = store();
    let old = conv();
    let new = conv();
    let mut rows = vec![
        summary(old, ConversationKind::Direct),
        summary(new, ConversationKind::Group),
    ];
    rows[0].last_at = 1;
    rows[0].last_seq = 10;
    rows[1].last_at = 2;
    rows[1].last_seq = 20;
    store.put_conversations(&rows).unwrap();
    assert_eq!(store.known(64).unwrap(), vec![(new, 20), (old, 10)]);
    assert_eq!(store.known(1).unwrap(), vec![(new, 20)]);
}

fn previewed(text: &str, seq: i64) -> MessagePreview {
    MessagePreview {
        seq,
        sender: PEER.into(),
        kind: PreviewKind::Text,
        text: text.into(),
        ts: seq * 1_000,
        expires_at: None,
    }
}

#[test]
fn a_gone_message_takes_its_preview_text_along() {
    let store = store();
    let expiring = conv();
    let hidden = conv();
    let kept = conv();
    for (id, seq) in [(expiring, 1), (hidden, 2), (kept, 3)] {
        let mut row = summary(id, ConversationKind::Group);
        row.preview = Some(previewed("тайное превью", seq));
        store.put_conversations(&[row]).unwrap();
    }
    let mut timed = post(expiring, 1, PEER, "тайное превью");
    timed.expires_at = Some(5_000);
    store
        .put_messages(&[
            timed,
            post(hidden, 1, PEER, "раньше"),
            post(hidden, 2, PEER, "тайное превью"),
        ])
        .unwrap();

    store.purge_expired(5_000).unwrap();
    store.hide_before(hidden, 2).unwrap();
    store
        .purge_messages(&[GoneMessage {
            conversation_id: kept,
            seq: 1,
        }])
        .unwrap();

    assert!(
        store
            .conversation(expiring)
            .unwrap()
            .unwrap()
            .preview
            .is_none()
    );
    assert!(
        store
            .conversation(hidden)
            .unwrap()
            .unwrap()
            .preview
            .is_none()
    );
    assert_eq!(
        store.conversation(kept).unwrap().unwrap().preview,
        Some(previewed("тайное превью", 3))
    );
}
