use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sc_chat::{Tier, Transport, TransportError, TransportRequest, TransportResponse};

const GRACE_MS: u64 = 5_000;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum NetFailure {
    #[error("the request timed out")]
    Timeout,
    #[error("the tier is unreachable")]
    Unreachable,
    #[error("the request was cancelled")]
    Cancelled,
    #[error("{reason}")]
    Other { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for NetFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Other {
            reason: error.reason,
        }
    }
}

impl From<NetFailure> for TransportError {
    fn from(failure: NetFailure) -> Self {
        match failure {
            NetFailure::Timeout => Self::Timeout,
            NetFailure::Unreachable => Self::Unreachable,
            NetFailure::Cancelled => Self::Cancelled,
            NetFailure::Other { reason } => Self::Other(reason),
        }
    }
}

#[uniffi::export(with_foreign)]
#[async_trait]
pub trait ChatTransport: Send + Sync {
    fn tiers(&self) -> Vec<Tier>;

    async fn exchange(&self, request: TransportRequest) -> Result<TransportResponse, NetFailure>;
}

pub struct Wire(pub Arc<dyn ChatTransport>);

#[async_trait]
impl Transport for Wire {
    fn tiers(&self) -> Vec<Tier> {
        self.0.tiers()
    }

    async fn exchange(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let limit = Duration::from_millis(request.timeout_ms.saturating_add(GRACE_MS));
        match tokio::time::timeout(limit, self.0.exchange(request)).await {
            Ok(answer) => answer.map_err(TransportError::from),
            Err(_) => Err(TransportError::Timeout),
        }
    }
}
