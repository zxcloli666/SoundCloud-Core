mod answer;
mod exchange;
mod path;
mod routes;
pub mod wire;

use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use answer::{decode, error_of, header};
pub use exchange::{BindSigner, Exchange, SealedExchange, Signer};
pub use routes::SyncReply;

use crate::channel::{Channel, InnerAnswer, InnerCall};
use crate::contracts::{ChatError, ChatResult, DeviceId, Tier, Transport};
use crate::crypto::DeviceKeys;

pub const ANSWER_SLACK_MS: u64 = 15_000;
pub const CALL_TIMEOUT_MS: u64 = 20_000;

#[derive(Clone)]
pub struct Identity {
    pub session_id: String,
    pub device_id: DeviceId,
    pub keys: Arc<DeviceKeys>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallOptions {
    pub tier: Option<Tier>,
    pub timeout_ms: u64,
    pub long_poll: bool,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self {
            tier: None,
            timeout_ms: CALL_TIMEOUT_MS,
            long_poll: false,
        }
    }
}

pub struct Api {
    channel: Arc<Channel>,
    transport: Arc<dyn Transport>,
    exchange: Arc<dyn Exchange>,
    identity: RwLock<Option<Identity>>,
    preferred: Mutex<Option<Tier>>,
}

impl Api {
    pub fn new(channel: Arc<Channel>, transport: Arc<dyn Transport>) -> Self {
        let exchange = Arc::new(SealedExchange::new(channel.clone(), transport.clone()));
        Self::with_exchange(channel, transport, exchange)
    }

    pub fn with_exchange(
        channel: Arc<Channel>,
        transport: Arc<dyn Transport>,
        exchange: Arc<dyn Exchange>,
    ) -> Self {
        Self {
            channel,
            transport,
            exchange,
            identity: RwLock::new(None),
            preferred: Mutex::new(None),
        }
    }

    pub fn channel(&self) -> &Arc<Channel> {
        &self.channel
    }

    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.transport
    }

    pub fn identity(&self) -> Option<Identity> {
        self.identity
            .read()
            .ok()
            .and_then(|identity| identity.clone())
    }

    pub fn set_identity(&self, identity: Option<Identity>) {
        if let Ok(mut current) = self.identity.write() {
            *current = identity;
        }
    }

    pub fn note_tier(&self, tier: Tier) {
        if let Ok(mut preferred) = self.preferred.lock() {
            *preferred = Some(tier);
        }
    }

    pub fn tier_order(&self) -> Vec<Tier> {
        let mut tiers = self.transport.tiers();
        let preferred = self.preferred.lock().ok().and_then(|preferred| *preferred);
        if let Some(at) = preferred.and_then(|tier| tiers.iter().position(|t| *t == tier)) {
            let tier = tiers.remove(at);
            tiers.insert(0, tier);
        }
        tiers
    }

    pub async fn call(&self, call: InnerCall, options: CallOptions) -> ChatResult<InnerAnswer> {
        let identity = self.identity().ok_or(ChatError::NotReady)?;
        self.call_as(&call, &Signer::Device(identity), options)
            .await
    }

    pub async fn call_as(
        &self,
        call: &InnerCall,
        signer: &Signer,
        options: CallOptions,
    ) -> ChatResult<InnerAnswer> {
        let tiers = match options.tier {
            Some(tier) => vec![tier],
            None => self.tier_order(),
        };
        for tier in tiers {
            match self.attempt(call, signer, tier, &options).await {
                Err(ChatError::Offline) => continue,
                Ok(answer) => {
                    self.note_tier(tier);
                    return Ok(answer);
                }
                Err(error) => return Err(error),
            }
        }
        Err(ChatError::Offline)
    }

    async fn attempt(
        &self,
        call: &InnerCall,
        signer: &Signer,
        tier: Tier,
        options: &CallOptions,
    ) -> ChatResult<InnerAnswer> {
        let answer = self.exchange.exchange(call, signer, tier, options).await?;
        match error_of(&answer) {
            Some(ChatError::Clock { server_now }) if server_now > 0 => {
                self.channel.note_server_now(server_now);
                self.exchange.exchange(call, signer, tier, options).await
            }
            Some(ChatError::Replay) => self.exchange.exchange(call, signer, tier, options).await,
            _ => Ok(answer),
        }
    }

    pub async fn json<T, B>(
        &self,
        method: &str,
        path: &str,
        query: &str,
        body: Option<&B>,
        options: CallOptions,
    ) -> ChatResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync,
    {
        let answer = self
            .call(inner_call(method, path, query, body)?, options)
            .await?;
        decode(&answer)
    }

    pub async fn json_as<T, B>(
        &self,
        signer: &Signer,
        method: &str,
        path: &str,
        query: &str,
        body: Option<&B>,
    ) -> ChatResult<T>
    where
        T: DeserializeOwned,
        B: Serialize + Sync,
    {
        let call = inner_call(method, path, query, body)?;
        let answer = self.call_as(&call, signer, CallOptions::default()).await?;
        decode(&answer)
    }
}

pub fn inner_call<B: Serialize>(
    method: &str,
    path: &str,
    query: &str,
    body: Option<&B>,
) -> ChatResult<InnerCall> {
    let body = match body {
        Some(body) => serde_json::to_vec(body).map_err(|_| ChatError::Invalid)?,
        None => Vec::new(),
    };
    Ok(InnerCall {
        method: method.to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        body,
    })
}
