#[path = "../sync_support/mod.rs"]
mod sync_support;

mod commit;
mod queue;
mod routes;

use sc_chat::api::wire::MediaKind;
use sc_chat::sc_chat_wire::proto::{Attachment, Thumb};
use sc_chat::store::{NewUpload, Store};
use serde_json::{Value, json};
use sync_support::{START_MS, Step};
use uuid::Uuid;

const CONVERSATION: Uuid = Uuid::from_u128(0xc0);
const IMAGE: Uuid = Uuid::from_u128(0x1a);
const THUMB: Uuid = Uuid::from_u128(0x1b);
const QUOTA: u64 = 1_073_741_824;

fn put(bytes: u64) -> Value {
    json!({
        "url": "https://s3.scnative.dev/opaque?X-Amz-Signature=1",
        "headers": {"content-length": bytes.to_string(), "content-type": "application/octet-stream"},
    })
}

fn reserve_answer(image: Uuid, thumb: Uuid) -> Value {
    json!({
        "id": image,
        "put": put(151_552),
        "thumb": {"id": thumb, "put": put(24_576)},
        "expires_in_s": 600,
        "used": 176_128,
        "quota": QUOTA,
    })
}

fn error(status: u16, body: Value) -> Step {
    Step::Json(status, body)
}

fn queued(store: &Store) -> Uuid {
    let attachment = Attachment {
        size: 151_552,
        thumb: Some(Thumb {
            size: 24_576,
            ..Thumb::default()
        }),
        ..Attachment::default()
    };
    let upload = NewUpload {
        album: Uuid::new_v4(),
        position: 0,
        conversation_id: CONVERSATION,
        kind: MediaKind::Image,
        image_ref: "media/a/0.img".into(),
        thumb_ref: Some("media/a/0.thumb".into()),
        attachment,
    };
    store.enqueue_upload(&upload, START_MS).unwrap()
}
