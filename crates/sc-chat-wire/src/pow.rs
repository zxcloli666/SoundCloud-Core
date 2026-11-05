use blake2::{Blake2b512, Digest};
use equix::{EquiXBuilder, RuntimeOption, Solution, SolutionByteArray};
use sha2::Sha256;

use crate::WireError;
use crate::proto::Pow;

#[cfg(feature = "solve")]
mod solve;

#[cfg(feature = "solve")]
pub use solve::{Ticket, solve};

pub const POW_V: u32 = 1;
pub const SEED_LEN: usize = 32;
pub const NONCE_LEN: usize = 16;
pub const SOLUTION_LEN: usize = 16;

const DOMAIN: &[u8] = b"scd-pow/v1";

pub fn challenge(
    seed: &[u8; SEED_LEN],
    auth_pub: &[u8; 32],
    path: &str,
    nonce: &[u8; NONCE_LEN],
    effort: u32,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DOMAIN.len() + 32 + 32 + 32 + NONCE_LEN + 4);
    bytes.extend_from_slice(DOMAIN);
    bytes.extend_from_slice(seed);
    bytes.extend_from_slice(auth_pub);
    bytes.extend_from_slice(&Sha256::digest(path.as_bytes()));
    bytes.extend_from_slice(nonce);
    bytes.extend_from_slice(&effort.to_be_bytes());
    bytes
}

pub fn meets_effort(challenge: &[u8], solution: &SolutionByteArray, effort: u32) -> bool {
    let mut hash = Blake2b512::new();
    hash.update(challenge);
    hash.update(solution);
    let digest = hash.finalize();
    let r = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    u64::from(r) * u64::from(effort) <= u64::from(u32::MAX)
}

pub fn verify(pow: &Pow, auth_pub: &[u8], path: &str) -> Result<(), WireError> {
    if pow.v != POW_V || pow.effort < 1 {
        return Err(WireError::Pow);
    }
    let seed: &[u8; SEED_LEN] = pow.seed.as_slice().try_into().map_err(|_| WireError::Pow)?;
    let auth_pub: &[u8; 32] = auth_pub.try_into().map_err(|_| WireError::Pow)?;
    let nonce: &[u8; NONCE_LEN] = pow
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| WireError::Pow)?;
    let bytes: &SolutionByteArray = pow
        .solution
        .as_slice()
        .try_into()
        .map_err(|_| WireError::Pow)?;
    let solution = Solution::try_from_bytes(bytes).map_err(|_| WireError::Pow)?;
    let challenge = challenge(seed, auth_pub, path, nonce, pow.effort);
    EquiXBuilder::new()
        .runtime(RuntimeOption::InterpretOnly)
        .verify(&challenge, &solution)
        .map_err(|_| WireError::Pow)?;
    if !meets_effort(&challenge, bytes, pow.effort) {
        return Err(WireError::Pow);
    }
    Ok(())
}
