use sc_chat_wire::msg::MessageAad;
use sc_chat_wire::proto::{Envelope, envelope::Body};
use sc_chat_wire::seal::NONCE_LEN;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::contracts::{ChatError, ChatResult, ClientId, ConvId, DeviceId, MessageClass};

pub const BODY_MAX: usize = 48 * 1024;
pub const META_MAX: usize = 8 * 1024;

pub struct Ck(Zeroizing<[u8; 32]>);

impl Ck {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAt {
    pub conversation_id: ConvId,
    pub epoch: u32,
    pub sender_device: DeviceId,
    pub client_id: ClientId,
    pub class: MessageClass,
}

impl MessageAt {
    pub fn meta(conversation_id: ConvId, epoch: u32) -> Self {
        Self {
            conversation_id,
            epoch,
            sender_device: Uuid::nil(),
            client_id: Uuid::nil(),
            class: MessageClass::Meta,
        }
    }

    fn sealed_as(&self) -> Self {
        match self.class {
            MessageClass::Meta => Self::meta(self.conversation_id, self.epoch),
            _ => *self,
        }
    }

    fn aad(&self) -> MessageAad {
        MessageAad {
            conv: *self.conversation_id.as_bytes(),
            epoch: self.epoch,
            sender_device: *self.sender_device.as_bytes(),
            client_id: *self.client_id.as_bytes(),
            class: self.class.wire(),
        }
    }
}

pub fn seal_message(ck: &Ck, at: &MessageAt, envelope: &Envelope) -> ChatResult<Vec<u8>> {
    seal_at(ck, &at.sealed_as(), sc_chat_wire::random_bytes()?, envelope)
}

pub fn open_message(ck: &Ck, at: &MessageAt, body: &[u8]) -> ChatResult<Envelope> {
    open_at(ck, &at.sealed_as(), body)
}

pub fn seal_meta(
    ck: &Ck,
    conversation_id: ConvId,
    epoch: u32,
    envelope: &Envelope,
) -> ChatResult<Vec<u8>> {
    seal_message(ck, &MessageAt::meta(conversation_id, epoch), envelope)
}

pub fn open_meta(
    ck: &Ck,
    conversation_id: ConvId,
    epoch: u32,
    meta_ct: &[u8],
) -> ChatResult<Envelope> {
    open_message(ck, &MessageAt::meta(conversation_id, epoch), meta_ct)
}

pub(super) fn seal_at(
    ck: &Ck,
    at: &MessageAt,
    nonce: [u8; NONCE_LEN],
    envelope: &Envelope,
) -> ChatResult<Vec<u8>> {
    if at.class == MessageClass::System || !carries(at.class, envelope) {
        return Err(ChatError::Invalid);
    }
    let body = sc_chat_wire::msg::seal(ck.as_bytes(), &at.aad(), nonce, envelope)?;
    if body.len() > cap(at.class) {
        return Err(ChatError::Invalid);
    }
    Ok(body)
}

pub(super) fn open_at(ck: &Ck, at: &MessageAt, body: &[u8]) -> ChatResult<Envelope> {
    if at.class == MessageClass::System {
        return Err(ChatError::Invalid);
    }
    let envelope = sc_chat_wire::msg::open(ck.as_bytes(), &at.aad(), body)?;
    match carries(at.class, &envelope) {
        true => Ok(envelope),
        false => Err(ChatError::Protocol),
    }
}

fn carries(class: MessageClass, envelope: &Envelope) -> bool {
    envelope
        .body
        .as_ref()
        .is_none_or(|body| class_of(body) == class)
}

fn class_of(body: &Body) -> MessageClass {
    match body {
        Body::Text(_) | Body::Suggest(_) => MessageClass::Post,
        Body::Edit(_) | Body::Delete(_) => MessageClass::Edit,
        Body::Reaction(_) => MessageClass::React,
        Body::Pin(_) => MessageClass::Pin,
        Body::Meta(_) => MessageClass::Meta,
    }
}

fn cap(class: MessageClass) -> usize {
    match class {
        MessageClass::Meta => META_MAX,
        _ => BODY_MAX,
    }
}
