mod store_support;
mod sync_support;

use sc_chat::api::wire::{ConversationList, ConversationView};
use sc_chat::{ChatError, ConversationBackdrop, ConversationSummary, RoomBackdrop};
use serde_json::{Value, json};
use store_support::summary;
use sync_support::fixtures::view;
use sync_support::real::real;
use uuid::Uuid;

const A: Uuid = Uuid::from_u128(0xa);
const MEDIA: Uuid = Uuid::from_u128(0x61);

fn parsed(backdrop: Option<Value>) -> Option<ConversationBackdrop> {
    let mut raw = view(A, 0);
    if let Some(backdrop) = backdrop {
        raw["backdrop"] = backdrop;
    }
    let view: ConversationView = serde_json::from_value(raw).unwrap();
    view.backdrop
}

fn preset(id: &str, by: &str) -> ConversationBackdrop {
    ConversationBackdrop {
        look: RoomBackdrop::Preset { id: id.into() },
        by: by.into(),
    }
}

#[test]
fn a_view_carries_a_preset_or_an_image_with_its_setter() {
    assert_eq!(
        parsed(Some(json!({"kind": "preset", "id": "aurora", "by": "7"}))),
        Some(preset("aurora", "7"))
    );
    let image = ConversationBackdrop {
        look: RoomBackdrop::Image { media_id: MEDIA },
        by: "8".into(),
    };
    assert_eq!(
        parsed(Some(json!({"kind": "image", "mediaId": MEDIA, "by": "8"}))),
        Some(image.clone())
    );
    assert_eq!(
        parsed(Some(json!({"kind": "image", "media_id": MEDIA, "by": "8"}))),
        Some(image)
    );
}

#[test]
fn an_old_server_or_an_unknown_kind_reads_as_no_backdrop() {
    assert_eq!(parsed(None), None);
    assert_eq!(parsed(Some(Value::Null)), None);
    assert_eq!(
        parsed(Some(json!({"kind": "video", "id": 3, "by": "7"}))),
        None
    );
    assert_eq!(
        parsed(Some(json!({"kind": "preset", "id": "aurora"}))),
        None
    );
    assert_eq!(parsed(Some(json!("aurora"))), None);
    let list: ConversationList = serde_json::from_value(json!({
        "items": [view(A, 0)],
        "next_cursor": null,
    }))
    .unwrap();
    assert_eq!(list.items[0].backdrop, None);
}

#[test]
fn a_backdrop_writes_as_the_server_does_and_a_summary_reads_without_one() {
    let shown = serde_json::to_value(preset("aurora", "7")).unwrap();
    assert_eq!(shown, json!({"kind": "preset", "id": "aurora", "by": "7"}));

    let mut with = summary(A, sc_chat::ConversationKind::Direct);
    with.backdrop = Some(preset("satin", "7"));
    let mut raw = serde_json::to_value(&with).unwrap();
    assert_eq!(
        raw["backdrop"],
        json!({"kind": "preset", "id": "satin", "by": "7"})
    );
    raw.as_object_mut().unwrap().remove("backdrop");
    let without: ConversationSummary = serde_json::from_value(raw).unwrap();
    assert_eq!(without.backdrop, None);
}

#[test]
fn a_star_refusal_has_its_own_error() {
    let refused = ChatError::from_server(&json!({"error": "chat_backdrop_star_required"}));
    assert_eq!(refused, ChatError::BackdropStarRequired);
    assert_eq!(
        serde_json::to_value(&refused).unwrap(),
        json!({"code": "backdrop_star_required"})
    );
}

#[tokio::test]
async fn a_refreshed_view_lands_its_backdrop_on_the_stored_summary() {
    let rig = real(&[]);
    let mut bare = view(A, 0);
    bare["peer"] = Value::Null;
    let mut raw = bare.clone();
    raw["backdrop"] = json!({"kind": "image", "mediaId": MEDIA, "by": "7"});
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), raw);
    rig.server
        .json("GET", &format!("/chat/conversations/{A}"), bare);

    let summary = rig.engine.refresh_conversation(A).await.unwrap();
    assert_eq!(
        summary.backdrop,
        Some(ConversationBackdrop {
            look: RoomBackdrop::Image { media_id: MEDIA },
            by: "7".into(),
        })
    );
    let stored = rig.engine.store().conversation(A).unwrap().unwrap();
    assert_eq!(stored.backdrop, summary.backdrop);

    let cleared = rig.engine.refresh_conversation(A).await.unwrap();
    assert_eq!(cleared.backdrop, None);
    let stored = rig.engine.store().conversation(A).unwrap().unwrap();
    assert_eq!(stored.backdrop, None);
}
