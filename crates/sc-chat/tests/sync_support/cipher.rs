use sc_chat::crypto::{Ck, MessageAt};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::Envelope;
use sc_chat::sync::Cipher;
use sc_chat::{ChatError, ChatResult, ConvId};

pub struct TestCipher;

pub fn ck_bytes(conversation_id: ConvId, epoch: u32) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(conversation_id.as_bytes());
    bytes[16..20].copy_from_slice(&epoch.to_be_bytes());
    bytes
}

pub fn sealed_ck(conversation_id: ConvId, epoch: u32) -> Vec<u8> {
    let mut sealed = b"ck:".to_vec();
    sealed.extend_from_slice(&ck_bytes(conversation_id, epoch));
    sealed
}

pub fn seal_body(conversation_id: ConvId, epoch: u32, envelope: &Envelope) -> Vec<u8> {
    let mut body = ck_bytes(conversation_id, epoch)[..20].to_vec();
    body.extend_from_slice(&envelope.encode_to_vec());
    body
}

impl Cipher for TestCipher {
    fn open_ck(&self, _conversation_id: ConvId, _epoch: u32, sealed: &[u8]) -> ChatResult<Ck> {
        let bytes = sealed.strip_prefix(b"ck:").ok_or(ChatError::Crypto)?;
        Ok(Ck::from_bytes(
            bytes.try_into().map_err(|_| ChatError::Crypto)?,
        ))
    }

    fn open_message(&self, ck: &Ck, at: &MessageAt, body: &[u8]) -> ChatResult<Envelope> {
        let expected = ck_bytes(at.conversation_id, at.epoch);
        if ck.as_bytes() != &expected || body.len() < 20 || body[..20] != expected[..20] {
            return Err(ChatError::Crypto);
        }
        Envelope::decode(&body[20..]).map_err(|_| ChatError::Crypto)
    }

    fn seal_message(&self, ck: &Ck, at: &MessageAt, envelope: &Envelope) -> ChatResult<Vec<u8>> {
        if ck.as_bytes() != &ck_bytes(at.conversation_id, at.epoch) {
            return Err(ChatError::Crypto);
        }
        Ok(seal_body(at.conversation_id, at.epoch, envelope))
    }

    fn open_meta(
        &self,
        ck: &Ck,
        conversation_id: ConvId,
        epoch: u32,
        meta_ct: &[u8],
    ) -> ChatResult<Envelope> {
        let expected = ck_bytes(conversation_id, epoch);
        if ck.as_bytes() != &expected || meta_ct.len() < 20 || meta_ct[..20] != expected[..20] {
            return Err(ChatError::Crypto);
        }
        Envelope::decode(&meta_ct[20..]).map_err(|_| ChatError::Crypto)
    }
}
