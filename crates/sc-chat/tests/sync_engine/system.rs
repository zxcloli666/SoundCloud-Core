use sc_chat::SystemKind;
use sc_chat::store::StoredContent;
use sc_chat::sync::Local;
use serde_json::json;

use crate::sync_support::fixtures::{answer, card, entry, summary, system, view};
use crate::sync_support::{ME, PEER, rig};
use crate::{A, INC, plan, synced};

#[tokio::test]
async fn system_rows_of_both_shapes_are_parsed() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            2,
            INC,
            vec![entry(
                A,
                2,
                vec![
                    system(1, json!({"kind": "ttl", "actor": PEER, "ttl_s": 86400})),
                    system(2, json!({"type": "kick", "user_id": ME, "name": "me", "by": PEER, "ban": true})),
                ],
            )],
        ),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 2));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    let stored = rig.local.stored(A);
    let StoredContent::System(ttl) = &stored[0].content else {
        panic!("not a system row");
    };
    assert_eq!(
        (ttl.kind, ttl.ttl_s, ttl.actor.as_deref()),
        (SystemKind::Ttl, Some(86400), Some(PEER))
    );
    let StoredContent::System(kick) = &stored[1].content else {
        panic!("not a system row");
    };
    assert_eq!(kick.kind, SystemKind::Banned);
    assert_eq!(
        (kick.actor.as_deref(), kick.target.as_deref()),
        (Some(PEER), Some(ME))
    );
    assert_eq!(rig.local.summary(A).unwrap().unread, 0);
}

#[tokio::test]
async fn role_delete_moderator_and_suggest_rows_keep_their_kind() {
    let rig = rig();
    synced(&rig);
    rig.local.put_conversations(&[summary(A, 0)]).unwrap();
    let track = json!({"urn": "soundcloud:tracks:1", "title": "Nightcall", "artist": "Kavinsky"});
    rig.server.json(
        "POST",
        "/chat/sync",
        answer(
            4,
            INC,
            vec![entry(
                A,
                4,
                vec![
                    system(1, json!({"kind": "role", "actor": PEER, "target": ME, "role": "admin"})),
                    system(2, json!({"kind": "deleted", "actor": PEER})),
                    system(3, json!({"type": "moderator", "user_id": ME, "name": "me", "by": PEER, "on": false})),
                    system(4, json!({"type": "suggest", "user_id": PEER, "name": "peer", "track": track})),
                ],
            )],
        ),
    );
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), view(A, 4));
    rig.server
        .json("GET", "/chat/users/200/card", card(PEER, "peer"));

    rig.engine.step(plan(&rig)).await.unwrap();

    let events: Vec<sc_chat::SystemEvent> = rig
        .local
        .stored(A)
        .into_iter()
        .filter_map(|row| match row.content {
            StoredContent::System(event) => Some(event),
            _ => None,
        })
        .collect();
    let kinds: Vec<SystemKind> = events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            SystemKind::Role,
            SystemKind::Deleted,
            SystemKind::Moderator,
            SystemKind::Suggest
        ]
    );
    assert_eq!(events[0].role, Some(sc_chat::MemberRole::Admin));
    assert_eq!(events[0].target.as_deref(), Some(ME));
    assert_eq!(
        (events[2].on, events[2].target.as_deref()),
        (Some(false), Some(ME))
    );
    assert_eq!(
        events[3].track.as_ref().map(|track| track.title.as_str()),
        Some("Nightcall")
    );
}
