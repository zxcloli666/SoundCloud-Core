use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::contracts::{ChatError, ChatResult, ConvId, Seq};

pub const NONCE_LEN: usize = 24;

pub struct Sealed {
    pub nonce: Vec<u8>,
    pub body: Vec<u8>,
}

pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> ChatResult<Sealed> {
    let nonce = sc_chat_wire::random_bytes::<NONCE_LEN>().map_err(|_| ChatError::Crypto)?;
    let body = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| ChatError::Crypto)?;
    Ok(Sealed {
        nonce: nonce.to_vec(),
        body,
    })
}

pub fn open(
    key: &[u8; 32],
    aad: &[u8],
    nonce: &[u8],
    body: &[u8],
) -> ChatResult<Zeroizing<Vec<u8>>> {
    if nonce.len() != NONCE_LEN {
        return Err(ChatError::Crypto);
    }
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: body, aad })
        .map(Zeroizing::new)
        .map_err(|_| ChatError::Crypto)
}

pub fn message_aad(conversation_id: ConvId, seq: Seq) -> Vec<u8> {
    let mut aad = conversation_id.as_bytes().to_vec();
    aad.extend_from_slice(&seq.to_be_bytes());
    aad
}

pub fn tagged_aad(tag: &str, parts: &[&[u8]]) -> Vec<u8> {
    let mut aad = format!("chat-store/{tag}\n").into_bytes();
    for part in parts {
        aad.extend_from_slice(&(part.len() as u32).to_be_bytes());
        aad.extend_from_slice(part);
    }
    aad
}
