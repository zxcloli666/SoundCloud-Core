use std::sync::Arc;

use sc_chat_wire::proto::Envelope;

use crate::api::Api;
use crate::contracts::{ChatError, ChatResult, ConvId};
use crate::crypto::{self, Ck, MessageAt};

pub trait Cipher: Send + Sync {
    fn open_ck(&self, conversation_id: ConvId, epoch: u32, sealed: &[u8]) -> ChatResult<Ck>;
    fn open_message(&self, ck: &Ck, at: &MessageAt, body: &[u8]) -> ChatResult<Envelope>;
    fn seal_message(&self, ck: &Ck, at: &MessageAt, envelope: &Envelope) -> ChatResult<Vec<u8>>;
    fn open_meta(
        &self,
        ck: &Ck,
        conversation_id: ConvId,
        epoch: u32,
        meta_ct: &[u8],
    ) -> ChatResult<Envelope>;
}

pub struct DeviceCipher {
    api: Arc<Api>,
}

impl DeviceCipher {
    pub fn new(api: Arc<Api>) -> Self {
        Self { api }
    }
}

impl Cipher for DeviceCipher {
    fn open_ck(&self, conversation_id: ConvId, epoch: u32, sealed: &[u8]) -> ChatResult<Ck> {
        let identity = self.api.identity().ok_or(ChatError::NotReady)?;
        identity
            .keys
            .open_ck(conversation_id, epoch, identity.device_id, sealed)
    }

    fn open_message(&self, ck: &Ck, at: &MessageAt, body: &[u8]) -> ChatResult<Envelope> {
        crypto::open_message(ck, at, body)
    }

    fn seal_message(&self, ck: &Ck, at: &MessageAt, envelope: &Envelope) -> ChatResult<Vec<u8>> {
        crypto::seal_message(ck, at, envelope)
    }

    fn open_meta(
        &self,
        ck: &Ck,
        conversation_id: ConvId,
        epoch: u32,
        meta_ct: &[u8],
    ) -> ChatResult<Envelope> {
        crypto::open_meta(ck, conversation_id, epoch, meta_ct)
    }
}
