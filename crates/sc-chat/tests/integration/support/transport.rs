use std::io::ErrorKind;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use std::sync::Arc;

use sc_chat::api::{CallOptions, Exchange, SealedExchange, Signer};
use sc_chat::channel::{AUTH_PATH, CHANNEL_PATH, Channel, InnerAnswer, InnerCall};
use sc_chat::login::is_login_path;
use sc_chat::{
    ChatResult, Tier, Transport, TransportError, TransportRequest, TransportResponse, async_trait,
};

use super::http::{self, Request};

pub struct HttpTransport {
    path: &'static str,
    routes: Vec<(Tier, String)>,
    sent: Mutex<Vec<(Tier, Vec<u8>)>>,
    losing: AtomicBool,
}

impl HttpTransport {
    pub fn new(routes: Vec<(Tier, String)>) -> Self {
        Self::at(CHANNEL_PATH, routes)
    }

    pub fn at(path: &'static str, routes: Vec<(Tier, String)>) -> Self {
        Self {
            path,
            routes,
            sent: Mutex::new(Vec::new()),
            losing: AtomicBool::new(false),
        }
    }

    pub fn lose_answers(&self, losing: bool) {
        self.losing.store(losing, Ordering::Relaxed);
    }

    pub fn sent(&self) -> Vec<(Tier, Vec<u8>)> {
        self.sent.lock().unwrap().clone()
    }
}

#[async_trait]
impl Transport for HttpTransport {
    fn tiers(&self) -> Vec<Tier> {
        self.routes.iter().map(|(tier, _)| *tier).collect()
    }

    async fn exchange(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let origin = self
            .routes
            .iter()
            .find(|(tier, _)| *tier == request.tier)
            .map(|(_, origin)| origin.clone())
            .ok_or(TransportError::Unreachable)?;
        self.sent
            .lock()
            .unwrap()
            .push((request.tier, request.envelope.clone()));
        let sent = http::send(
            &origin,
            Request {
                method: "POST",
                path: self.path,
                headers: vec![("content-type", "application/octet-stream".to_owned())],
                body: request.envelope,
                timeout: Duration::from_millis(request.timeout_ms),
            },
        )
        .await;
        let reply = match sent {
            Ok(_) if self.losing.load(Ordering::Relaxed) => return Err(TransportError::Timeout),
            Ok(reply) => reply,
            Err(error) if error.kind() == ErrorKind::TimedOut => {
                return Err(TransportError::Timeout);
            }
            Err(_) => return Err(TransportError::Unreachable),
        };
        let retry_after_ms = reply
            .header("retry-after")
            .and_then(|seconds| seconds.trim().parse::<u64>().ok())
            .map(|seconds| seconds * 1_000);
        Ok(TransportResponse {
            status: reply.status,
            body: reply.body,
            retry_after_ms,
        })
    }
}

pub struct SplitExchange {
    chat: SealedExchange,
    auth: SealedExchange,
}

impl SplitExchange {
    pub fn new(
        channel: Arc<Channel>,
        chat: Arc<HttpTransport>,
        routes: Vec<(Tier, String)>,
    ) -> Self {
        let auth = Arc::new(HttpTransport::at(AUTH_PATH, routes));
        Self {
            chat: SealedExchange::new(channel.clone(), chat),
            auth: SealedExchange::new(channel, auth),
        }
    }
}

#[async_trait]
impl Exchange for SplitExchange {
    async fn exchange(
        &self,
        call: &InnerCall,
        signer: &Signer,
        tier: Tier,
        options: &CallOptions,
    ) -> ChatResult<InnerAnswer> {
        let side = if is_login_path(&call.path) {
            &self.auth
        } else {
            &self.chat
        };
        side.exchange(call, signer, tier, options).await
    }
}
