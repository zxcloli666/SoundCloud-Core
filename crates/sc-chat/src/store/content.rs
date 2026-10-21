use sc_chat_wire::prost::Message as _;
use sc_chat_wire::proto::Envelope;

use crate::envelope::{self, Content};

pub fn decode(bytes: &[u8]) -> Content {
    Envelope::decode(bytes)
        .ok()
        .and_then(|envelope| envelope::decode(&envelope).ok())
        .unwrap_or(Content::Unknown)
}
