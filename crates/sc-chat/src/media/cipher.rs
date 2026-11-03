use zeroize::Zeroizing;

use super::MediaError;
use crate::store::seal;

pub const BLOCK: usize = 4096;
const TAG: usize = 16;
const PAD_MARK: u8 = 0x80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Image,
    Thumb,
}

impl Role {
    fn aad(self) -> Vec<u8> {
        let label: &[u8] = match self {
            Self::Image => b"image",
            Self::Thumb => b"thumb",
        };
        seal::tagged_aad("media/v1", &[label])
    }
}

pub struct Blob {
    pub ciphertext: Vec<u8>,
    pub key: Zeroizing<[u8; 32]>,
    pub nonce: Vec<u8>,
}

pub fn sealed_len(plain_len: usize) -> usize {
    (plain_len + 1 + TAG).div_ceil(BLOCK) * BLOCK
}

pub fn seal_blob(role: Role, plain: &[u8]) -> Result<Blob, MediaError> {
    let key = Zeroizing::new(sc_chat_wire::random_bytes::<32>().map_err(|_| MediaError::Random)?);
    let mut padded = Zeroizing::new(vec![0u8; sealed_len(plain.len()) - TAG]);
    padded[..plain.len()].copy_from_slice(plain);
    padded[plain.len()] = PAD_MARK;
    let sealed = seal::seal(&key, &role.aad(), &padded).map_err(|_| MediaError::Cipher)?;
    Ok(Blob {
        ciphertext: sealed.body,
        key,
        nonce: sealed.nonce,
    })
}

pub fn open_blob(
    role: Role,
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, MediaError> {
    let key: &[u8; 32] = key.try_into().map_err(|_| MediaError::Cipher)?;
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(BLOCK) {
        return Err(MediaError::Cipher);
    }
    let mut plain =
        seal::open(key, &role.aad(), nonce, ciphertext).map_err(|_| MediaError::Cipher)?;
    let end = plain
        .iter()
        .rposition(|byte| *byte != 0)
        .filter(|at| plain[*at] == PAD_MARK)
        .ok_or(MediaError::Cipher)?;
    plain.truncate(end);
    Ok(plain)
}
