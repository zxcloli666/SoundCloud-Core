#[path = "../sync_support/mod.rs"]
mod sync_support;

mod gone;
mod marks;
mod polls;
mod reset;
mod rooms;
mod system;

use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::store::{StoredContent, StoredMessage, SyncState};
use sc_chat::sync::{Local, PollPlan};
use sc_chat::{MessageClass, Tier};
use sync_support::fixtures::text;
use sync_support::{PEER, PEER_DEVICE, START_MS};
use uuid::Uuid;

const INC: Uuid = Uuid::from_u128(0x1c);
const A: Uuid = Uuid::from_u128(0xa);
const B: Uuid = Uuid::from_u128(0xb);
const C: Uuid = Uuid::from_u128(0xc);
const ROOM: Uuid = Uuid::from_u128(0xe);

fn synced(rig: &sync_support::Rig) {
    rig.local
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
}

fn open(conversation_id: Uuid, seq: i64, body: &str) -> StoredMessage {
    StoredMessage {
        conversation_id,
        seq,
        client_id: Uuid::from_u128(seq as u128 + 0x1000),
        sender: PEER.into(),
        sender_device: Some(PEER_DEVICE),
        class: MessageClass::Post,
        epoch: 1,
        target_seq: None,
        ts: START_MS + seq * 1000,
        expires_at: None,
        local_only: false,
        content: StoredContent::Open(text(body).encode_to_vec()),
    }
}

fn plan(rig: &sync_support::Rig) -> PollPlan {
    rig.engine.plan(Tier::Direct)
}
