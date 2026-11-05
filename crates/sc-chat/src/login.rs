use sc_chat_wire::pow::POW_V;
use sc_chat_wire::proto::{Inner, Pow};

use crate::channel::{Channel, InnerCall, Sealed};
use crate::contracts::{ChatError, ChatResult, DeviceId, Tier};
use crate::crypto::DeviceKeys;

pub const LOGIN_PATHS: [&str; 5] = [
    "/auth/login",
    "/auth/login/status",
    "/auth/link/create",
    "/auth/link/claim",
    "/auth/link/status",
];

pub const LOGIN_MAX_BYTES: u32 = 64 * 1024;

pub fn is_login_path(path: &str) -> bool {
    LOGIN_PATHS.contains(&path)
}

pub fn seal_login(
    channel: &Channel,
    keys: &DeviceKeys,
    device_id: DeviceId,
    call: &InnerCall,
    target_session: Option<&str>,
    tier: Tier,
    pow: Option<&Pow>,
) -> ChatResult<Sealed> {
    let bind = keys.bind(device_id, channel.wire_now())?;
    let pow = pow.cloned().unwrap_or(Pow {
        v: POW_V,
        ..Pow::default()
    });
    let extra = Inner {
        session_id: target_session.map(str::to_owned),
        bind: Some(bind),
        pow: Some(pow),
        ..Inner::default()
    };
    seal_bound(
        channel,
        keys,
        call,
        tier,
        extra,
        sc_chat_wire::random_bytes()?,
    )
}

pub(crate) fn seal_bound(
    channel: &Channel,
    keys: &DeviceKeys,
    call: &InnerCall,
    tier: Tier,
    extra: Inner,
    ikm_e: [u8; 32],
) -> ChatResult<Sealed> {
    if !is_login_path(&call.path) {
        return Err(ChatError::Invalid);
    }
    let inner = Inner {
        session_id: extra.session_id,
        bind: extra.bind,
        pow: extra.pow,
        ..channel.inner(call, tier, LOGIN_MAX_BYTES.min(tier.max_bytes()))
    };
    channel.seal_signed(inner, keys, ikm_e)
}
