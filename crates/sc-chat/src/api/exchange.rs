use std::sync::Arc;

use async_trait::async_trait;

use super::{CallOptions, Identity};
use crate::channel::{Caller, Channel, InnerAnswer, InnerCall, Sealed};
use crate::contracts::{ChatError, ChatResult, DeviceId, Tier, Transport, TransportRequest};
use crate::crypto::DeviceKeys;
use crate::login::seal_login;

const OK: u16 = 200;
const BUSY: u16 = 503;

#[derive(Clone)]
pub struct BindSigner {
    pub device_id: DeviceId,
    pub keys: Arc<DeviceKeys>,
    pub target_session: Option<String>,
}

#[derive(Clone)]
pub enum Signer {
    Device(Identity),
    Bind(BindSigner),
}

#[async_trait]
pub trait Exchange: Send + Sync {
    async fn exchange(
        &self,
        call: &InnerCall,
        signer: &Signer,
        tier: Tier,
        options: &CallOptions,
    ) -> ChatResult<InnerAnswer>;
}

pub struct SealedExchange {
    channel: Arc<Channel>,
    transport: Arc<dyn Transport>,
}

impl SealedExchange {
    pub fn new(channel: Arc<Channel>, transport: Arc<dyn Transport>) -> Self {
        Self { channel, transport }
    }

    fn seal(&self, call: &InnerCall, signer: &Signer, tier: Tier) -> ChatResult<Sealed> {
        match signer {
            Signer::Device(identity) => {
                let caller = Caller {
                    session_id: &identity.session_id,
                    device_id: identity.device_id,
                    keys: &identity.keys,
                };
                self.channel.seal(call, &caller, tier)
            }
            Signer::Bind(bind) => seal_login(
                &self.channel,
                &bind.keys,
                bind.device_id,
                call,
                bind.target_session.as_deref(),
                tier,
            ),
        }
    }
}

#[async_trait]
impl Exchange for SealedExchange {
    async fn exchange(
        &self,
        call: &InnerCall,
        signer: &Signer,
        tier: Tier,
        options: &CallOptions,
    ) -> ChatResult<InnerAnswer> {
        let mut sealed = self.seal(call, signer, tier)?;
        let request = TransportRequest {
            tier,
            envelope: std::mem::take(&mut sealed.envelope),
            timeout_ms: options.timeout_ms,
            long_poll: options.long_poll,
        };
        let response = self.transport.exchange(request).await?;
        if response.status == BUSY && response.retry_after_ms.is_none() {
            return Err(ChatError::Offline);
        }
        match self.channel.receive(&sealed, &response) {
            Err(ChatError::Crypto | ChatError::Protocol) if response.status == OK => {
                Err(ChatError::Offline)
            }
            received => received,
        }
    }
}
