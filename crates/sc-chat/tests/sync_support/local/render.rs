use sc_chat::embeds::wire::from_proto;
use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::{Envelope, envelope::Body};
use sc_chat::store::{StoredContent, StoredMessage};
use sc_chat::{DeliveryState, Message, MessageBody, MessageClass, RichText};

use super::MemoryLocal;

impl MemoryLocal {
    pub(super) fn render(&self, stored: &StoredMessage) -> Message {
        let mut embeds = Vec::new();
        let body = match &stored.content {
            StoredContent::Open(bytes) => match Envelope::decode(bytes.as_slice()) {
                Ok(envelope) => {
                    embeds = envelope.embeds.iter().filter_map(from_proto).collect();
                    match envelope.body {
                        Some(Body::Text(text)) => MessageBody::Text(RichText {
                            text: text.text,
                            spans: Vec::new(),
                        }),
                        _ => MessageBody::Unknown,
                    }
                }
                Err(_) => MessageBody::Unknown,
            },
            StoredContent::Sealed(_) => MessageBody::Locked,
            StoredContent::System(event) => MessageBody::System(event.clone()),
            StoredContent::Deleted => MessageBody::Deleted,
        };
        Message {
            conversation_id: stored.conversation_id,
            seq: Some(stored.seq),
            client_id: stored.client_id,
            sender: stored.sender.clone(),
            sender_device: stored.sender_device,
            mine: stored.sender == self.me,
            ts: stored.ts,
            edited_at: None,
            expires_at: stored.expires_at,
            state: DeliveryState::Sent,
            body,
            reply_to: None,
            mentions: Vec::new(),
            mentions_me: false,
            embeds,
            attachments: Vec::new(),
            forwarded_from: None,
            reactions: Vec::new(),
            pinned: false,
            local_only: stored.local_only,
        }
    }

    pub(super) fn pending(&self, item: &OutboxItem, failed: bool) -> Message {
        Message {
            conversation_id: item.conversation_id,
            seq: None,
            client_id: item.client_id,
            sender: self.me.clone(),
            sender_device: None,
            mine: true,
            ts: item.created_at,
            edited_at: None,
            expires_at: None,
            state: if failed {
                DeliveryState::Failed
            } else {
                DeliveryState::Pending
            },
            body: MessageBody::Unknown,
            reply_to: None,
            mentions: Vec::new(),
            mentions_me: false,
            embeds: Vec::new(),
            attachments: Vec::new(),
            forwarded_from: None,
            reactions: Vec::new(),
            pinned: false,
            local_only: false,
        }
    }
}

pub(super) fn visible(class: MessageClass) -> bool {
    matches!(class, MessageClass::Post | MessageClass::System)
}
