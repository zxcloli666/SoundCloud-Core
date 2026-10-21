use async_trait::async_trait;
use zeroize::Zeroizing;

use super::error::ChatResult;
use super::events::CoreEvent;
use super::ids::Millis;
use super::status::{KeyStorage, Tier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequest {
    pub tier: Tier,
    pub envelope: Vec<u8>,
    pub timeout_ms: u64,
    pub long_poll: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("the request timed out")]
    Timeout,
    #[error("the tier is unreachable")]
    Unreachable,
    #[error("the request was cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait Transport: Send + Sync {
    fn tiers(&self) -> Vec<Tier>;

    async fn exchange(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyName {
    Database,
    DeviceId,
    DeviceAuth,
    DeviceBox,
}

impl KeyName {
    pub const ALL: [KeyName; 4] = [
        KeyName::Database,
        KeyName::DeviceId,
        KeyName::DeviceAuth,
        KeyName::DeviceBox,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            KeyName::Database => "db",
            KeyName::DeviceId => "device-id",
            KeyName::DeviceAuth => "device-auth",
            KeyName::DeviceBox => "device-box",
        }
    }
}

pub trait KeyStore: Send + Sync {
    fn storage(&self) -> KeyStorage;

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>>;

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()>;

    fn delete(&self, name: KeyName) -> ChatResult<()>;
}

#[async_trait]
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> Millis;

    async fn sleep(&self, ms: u64);
}

pub trait Events: Send + Sync {
    fn emit(&self, event: CoreEvent);
}
