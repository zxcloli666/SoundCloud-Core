mod answer;
mod pins;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};

use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::Inner;
use sc_chat_wire::seal::{ENC_LEN, RequestSealer, ResponseKey};

use crate::contracts::{ChatError, ChatResult, Clock, DeviceId, Millis, Tier};
use crate::crypto::DeviceKeys;

pub use answer::{BUSY_RETRY_MAX_MS, BUSY_RETRY_MS, InnerAnswer};
pub use pins::{DEV_OCK_ENV, dev_pin};

pub const CHANNEL_PATH: &str = "/chat/x";

pub type OckPin = (u8, [u8; 32]);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerCall {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Vec<u8>,
}

pub struct Caller<'a> {
    pub session_id: &'a str,
    pub device_id: DeviceId,
    pub keys: &'a DeviceKeys,
}

pub struct Sealed {
    pub envelope: Vec<u8>,
    pub enc: [u8; ENC_LEN],
    pub response_key: ResponseKey,
}

pub struct Channel {
    clock: Arc<dyn Clock>,
    offset_ms: AtomicI64,
    ock_version: AtomicU8,
    pins: Vec<OckPin>,
}

impl Channel {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::with_pins(clock, pins::pinned())
    }

    pub fn with_pins(clock: Arc<dyn Clock>, pins: Vec<OckPin>) -> Self {
        let first = pins.first().map_or(0, |(version, _)| *version);
        Self {
            clock,
            offset_ms: AtomicI64::new(0),
            ock_version: AtomicU8::new(first),
            pins,
        }
    }

    pub fn now_ms(&self) -> Millis {
        self.clock.now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    pub fn offset_ms(&self) -> Millis {
        self.offset_ms.load(Ordering::Relaxed)
    }

    pub fn note_server_now(&self, server_now: Millis) {
        self.offset_ms
            .store(server_now - self.clock.now_ms(), Ordering::Relaxed);
    }

    pub fn ock_version(&self) -> u8 {
        self.ock_version.load(Ordering::Relaxed)
    }

    pub fn use_ock(&self, version: u8) -> bool {
        let pinned = self.pin(version).is_some();
        if pinned {
            self.ock_version.store(version, Ordering::Relaxed);
        }
        pinned
    }

    pub fn seal(&self, call: &InnerCall, caller: &Caller<'_>, tier: Tier) -> ChatResult<Sealed> {
        self.seal_call(call, caller, tier, sc_chat_wire::random_bytes()?)
    }

    pub(crate) fn seal_call(
        &self,
        call: &InnerCall,
        caller: &Caller<'_>,
        tier: Tier,
        ikm_e: [u8; 32],
    ) -> ChatResult<Sealed> {
        let inner = Inner {
            session_id: Some(caller.session_id.to_owned()),
            device_id: Some(caller.device_id.to_string()),
            ..self.inner(call, tier, tier.max_bytes())
        };
        self.seal_signed(inner, caller.keys, ikm_e)
    }

    pub(crate) fn inner(&self, call: &InnerCall, tier: Tier, max_bytes: u32) -> Inner {
        Inner {
            ts_ms: self.wire_now(),
            session_id: None,
            device_id: None,
            method: call.method.clone(),
            path: call.path.clone(),
            query: call.query.clone(),
            body: call.body.clone(),
            tier: tier.wire().into(),
            max_bytes,
            bind: None,
            sig: Vec::new(),
        }
    }

    pub(crate) fn seal_signed(
        &self,
        mut inner: Inner,
        keys: &DeviceKeys,
        ikm_e: [u8; 32],
    ) -> ChatResult<Sealed> {
        let version = self.ock_version();
        let ock = self.pin(version).ok_or(ChatError::Crypto)?;
        let sealer = RequestSealer::new(version, ock, ikm_e)?;
        keys.sign(&mut inner, sealer.enc())?;
        let sealed = sealer.seal(&inner.encode_to_vec())?;
        Ok(Sealed {
            envelope: sealed.envelope,
            enc: sealed.enc,
            response_key: sealed.response_key,
        })
    }

    pub(crate) fn wire_now(&self) -> u64 {
        u64::try_from(self.now_ms()).unwrap_or_default()
    }

    fn pin(&self, version: u8) -> Option<&[u8; 32]> {
        self.pins
            .iter()
            .find(|(pinned, _)| *pinned == version)
            .map(|(_, key)| key)
    }
}
