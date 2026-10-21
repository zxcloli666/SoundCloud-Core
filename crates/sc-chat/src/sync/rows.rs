use sc_chat_wire::prost::Message as _;

use super::cipher::Cipher;
use super::system;
use crate::api::wire::MessageView;
use crate::contracts::{ConvId, MessageClass};
use crate::crypto::{Ck, MessageAt};
use crate::embeds::wire::sanitize;
use crate::store::{StoredContent, StoredMessage};

pub fn epoch_of(view: &MessageView) -> Option<u32> {
    u32::try_from(view.epoch).ok()
}

pub fn needs_key(view: &MessageView) -> bool {
    view.body.is_some() && view.class != MessageClass::System.as_str()
}

pub fn stored(
    conversation_id: ConvId,
    view: &MessageView,
    ck: Option<&Ck>,
    cipher: &dyn Cipher,
) -> Option<StoredMessage> {
    let class = MessageClass::parse(&view.class)?;
    let epoch = epoch_of(view)?;
    let content = match (&view.body, class) {
        (None, _) => StoredContent::Deleted,
        (Some(body), MessageClass::System) => StoredContent::System(system::parse(body)),
        (Some(body), _) => StoredContent::Sealed(body.clone()),
    };
    let message = StoredMessage {
        conversation_id,
        seq: view.seq,
        client_id: view.client_id,
        sender: view.sender.clone(),
        sender_device: view.sender_device,
        class,
        epoch,
        target_seq: view.target_seq,
        ts: view.ts,
        expires_at: view.expires_at,
        local_only: false,
        content,
    };
    Some(match ck {
        Some(ck) => opened(message, ck, cipher),
        None => message,
    })
}

pub fn opened(mut message: StoredMessage, ck: &Ck, cipher: &dyn Cipher) -> StoredMessage {
    let (StoredContent::Sealed(body), Some(sender_device)) =
        (&message.content, message.sender_device)
    else {
        return message;
    };
    let at = MessageAt {
        conversation_id: message.conversation_id,
        epoch: message.epoch,
        sender_device,
        client_id: message.client_id,
        class: message.class,
    };
    if let Ok(mut envelope) = cipher.open_message(ck, &at, body) {
        sanitize(&mut envelope);
        envelope.padding.clear();
        message.content = StoredContent::Open(envelope.encode_to_vec());
    }
    message
}
