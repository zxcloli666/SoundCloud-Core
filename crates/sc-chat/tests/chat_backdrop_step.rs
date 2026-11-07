mod store_support;
mod sync_support;

use sc_chat::media::BackdropStep;
use sc_chat::store::Store;
use sc_chat::{BackdropChange, ChatError, ConversationKind, ConversationMeta};
use serde_json::{Value, json};
use store_support::{backdrop, committed_backdrop, conv, store, summary};
use sync_support::fixtures::view;
use sync_support::{Rig, Step, rig};
use uuid::Uuid;

const MEDIA: u128 = 0x61;
const AVATAR: u128 = 0x10;

fn seat(store: &Store, kind: ConversationKind) -> Uuid {
    let id = conv();
    store.put_conversations(&[summary(id, kind)]).unwrap();
    id
}

fn path(id: Uuid) -> String {
    format!("/chat/conversations/{id}/backdrop")
}

fn answers(rig: &Rig, id: Uuid, times: usize) {
    for _ in 0..times {
        rig.server.json("PUT", &path(id), view(id, 0));
    }
}

fn bodies(rig: &Rig, id: Uuid) -> Vec<Value> {
    rig.server
        .calls_to("PUT", &path(id))
        .into_iter()
        .map(|call| call.body)
        .collect()
}

fn image_body() -> Value {
    json!({"kind": "image", "mediaId": Uuid::from_u128(MEDIA)})
}

#[tokio::test]
async fn a_direct_chat_image_goes_to_the_backdrop_route_with_a_backdrop_only_meta() {
    let rig = rig();
    let store = store();
    let id = seat(&store, ConversationKind::Direct);
    let upload_id = committed_backdrop(&store, id, MEDIA);
    answers(&rig, id, 1);

    let step = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    let meta = step.send(rig.engine.api()).await.unwrap().unwrap();

    assert_eq!(
        meta,
        ConversationMeta {
            backdrop: Some(backdrop(MEDIA)),
            ..ConversationMeta::default()
        }
    );
    assert_eq!(meta.backdrop.unwrap().key, vec![7; 32]);
    assert_eq!(bodies(&rig, id), vec![image_body()]);
}

#[tokio::test]
async fn a_direct_saved_or_public_clear_and_preset_send_no_meta() {
    let rig = rig();
    let store = store();
    for kind in [
        ConversationKind::Direct,
        ConversationKind::Saved,
        ConversationKind::Public,
    ] {
        let id = seat(&store, kind);
        let shown = ConversationMeta {
            backdrop: Some(backdrop(MEDIA)),
            ..ConversationMeta::default()
        };
        store.put_group_meta(id, &shown).unwrap();
        answers(&rig, id, 2);

        let cleared = BackdropStep::set(&store, id, &BackdropChange::Clear).unwrap();
        assert_eq!(cleared.send(rig.engine.api()).await.unwrap(), None);
        let preset = BackdropStep::preset(&store, id, "aurora".into()).unwrap();
        assert_eq!(preset.send(rig.engine.api()).await.unwrap(), None);

        assert_eq!(
            bodies(&rig, id),
            vec![Value::Null, json!({"kind": "preset", "id": "aurora"})]
        );
    }
}

#[tokio::test]
async fn a_group_meta_keeps_its_avatar_next_to_the_backdrop() {
    let rig = rig();
    let store = store();
    let id = seat(&store, ConversationKind::Group);
    let upload_id = committed_backdrop(&store, id, MEDIA);
    let titled = ConversationMeta {
        title: "Ночная смена".into(),
        description: "d".into(),
        avatar: Some(backdrop(AVATAR)),
        backdrop: None,
    };
    store.put_group_meta(id, &titled).unwrap();
    answers(&rig, id, 3);

    let set = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    let with_both = set.send(rig.engine.api()).await.unwrap().unwrap();
    assert_eq!(
        with_both,
        ConversationMeta {
            backdrop: Some(backdrop(MEDIA)),
            ..titled.clone()
        }
    );
    store.put_group_meta(id, &with_both).unwrap();

    let preset = BackdropStep::preset(&store, id, "satin".into()).unwrap();
    assert_eq!(
        preset.send(rig.engine.api()).await.unwrap(),
        Some(titled.clone())
    );
    store.put_group_meta(id, &titled).unwrap();

    let again = BackdropStep::preset(&store, id, "dunes".into()).unwrap();
    assert_eq!(again.send(rig.engine.api()).await.unwrap(), None);
    assert_eq!(
        bodies(&rig, id),
        vec![
            image_body(),
            json!({"kind": "preset", "id": "satin"}),
            json!({"kind": "preset", "id": "dunes"}),
        ]
    );
}

#[tokio::test]
async fn a_group_without_a_known_meta_sends_a_titled_meta_only_for_an_image() {
    let rig = rig();
    let store = store();
    let id = conv();
    let mut seat = summary(id, ConversationKind::Group);
    seat.title = Some("Ночная смена".into());
    store.put_conversations(&[seat]).unwrap();
    let upload_id = committed_backdrop(&store, id, MEDIA);
    answers(&rig, id, 2);

    let clear = BackdropStep::set(&store, id, &BackdropChange::Clear).unwrap();
    assert_eq!(clear.send(rig.engine.api()).await.unwrap(), None);
    let set = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
    assert_eq!(
        set.send(rig.engine.api()).await.unwrap(),
        Some(ConversationMeta {
            title: "Ночная смена".into(),
            backdrop: Some(backdrop(MEDIA)),
            ..ConversationMeta::default()
        })
    );
}

#[tokio::test]
async fn a_refused_backdrop_keeps_its_typed_error_and_hands_back_no_meta() {
    let rig = rig();
    let store = store();
    let id = seat(&store, ConversationKind::Group);
    let upload_id = committed_backdrop(&store, id, MEDIA);
    let refusals = [
        (
            403,
            "chat_backdrop_star_required",
            ChatError::BackdropStarRequired,
        ),
        (403, "chat_not_member", ChatError::NotMember),
        (403, "chat_unavailable", ChatError::Unavailable),
        (429, "chat_request_limit", ChatError::RequestLimit),
        (400, "chat_invalid", ChatError::Invalid),
    ];
    for (status, code, _) in &refusals {
        rig.server.on(
            "PUT",
            &path(id),
            Step::Json(*status, json!({"error": code})),
        );
    }

    for (_, _, expected) in refusals {
        let step = BackdropStep::set(&store, id, &BackdropChange::Set { upload_id }).unwrap();
        assert_eq!(step.send(rig.engine.api()).await.unwrap_err(), expected);
    }
}

#[test]
fn an_unknown_chat_never_reaches_the_network() {
    let store = store();
    let odd = seat(&store, ConversationKind::Unknown);
    for id in [odd, conv()] {
        assert_eq!(
            BackdropStep::set(&store, id, &BackdropChange::Clear).err(),
            Some(ChatError::Invalid)
        );
        assert_eq!(
            BackdropStep::preset(&store, id, "aurora".into()).err(),
            Some(ChatError::Invalid)
        );
    }
}
