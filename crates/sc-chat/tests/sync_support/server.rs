use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use sc_chat::api::{CallOptions, Exchange, Signer};
use sc_chat::channel::{InnerAnswer, InnerCall};
use sc_chat::{
    ChatError, ChatResult, Tier, Transport, TransportError, TransportRequest, TransportResponse,
    async_trait,
};
use serde_json::Value;

use super::TestClock;

pub enum Step {
    Json(u16, Value),
    Empty(u16, Vec<(String, String)>),
    Fail(ChatError),
    FailAfter(u64, ChatError),
}

#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Value,
    pub tier: Tier,
    pub long_poll: bool,
    pub timeout_ms: u64,
    pub bound: bool,
}

type Hook = Box<dyn Fn() + Send + Sync>;

pub struct Server {
    tiers: Mutex<Vec<Tier>>,
    clock: Arc<TestClock>,
    script: Mutex<HashMap<String, VecDeque<Step>>>,
    calls: Mutex<Vec<Call>>,
    done: Mutex<Option<Hook>>,
}

impl Server {
    pub fn new(tiers: &[Tier], clock: Arc<TestClock>) -> Self {
        Self {
            tiers: Mutex::new(tiers.to_vec()),
            clock,
            script: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            done: Mutex::new(None),
        }
    }

    pub fn on(&self, method: &str, path: &str, step: Step) {
        self.script
            .lock()
            .unwrap()
            .entry(format!("{method} {path}"))
            .or_default()
            .push_back(step);
    }

    pub fn json(&self, method: &str, path: &str, body: Value) {
        self.on(method, path, Step::Json(200, body));
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    pub fn calls_to(&self, method: &str, path: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.method == method && call.path == path)
            .collect()
    }

    pub fn left(&self, method: &str, path: &str) -> usize {
        self.script
            .lock()
            .unwrap()
            .get(&format!("{method} {path}"))
            .map_or(0, VecDeque::len)
    }

    pub fn when_done(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.done.lock().unwrap() = Some(Box::new(hook));
    }

    fn next(&self, call: &InnerCall) -> Option<Step> {
        self.script
            .lock()
            .unwrap()
            .get_mut(&format!("{} {}", call.method, call.path))
            .and_then(VecDeque::pop_front)
    }
}

#[async_trait]
impl Transport for Server {
    fn tiers(&self) -> Vec<Tier> {
        self.tiers.lock().unwrap().clone()
    }

    async fn exchange(
        &self,
        _request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        Err(TransportError::Unreachable)
    }
}

#[async_trait]
impl Exchange for Server {
    async fn exchange(
        &self,
        call: &InnerCall,
        signer: &Signer,
        tier: Tier,
        options: &CallOptions,
    ) -> ChatResult<InnerAnswer> {
        self.calls.lock().unwrap().push(Call {
            method: call.method.clone(),
            path: call.path.clone(),
            query: call.query.clone(),
            body: serde_json::from_slice(&call.body).unwrap_or(Value::Null),
            tier,
            long_poll: options.long_poll,
            timeout_ms: options.timeout_ms,
            bound: matches!(signer, Signer::Bind(_)),
        });
        let Some(step) = self.next(call) else {
            if let Some(hook) = self.done.lock().unwrap().as_ref() {
                hook();
            }
            return Err(ChatError::Offline);
        };
        match step {
            Step::Json(status, body) => Ok(InnerAnswer {
                status,
                headers: Vec::new(),
                body: serde_json::to_vec(&body).unwrap(),
            }),
            Step::Empty(status, headers) => Ok(InnerAnswer {
                status,
                headers,
                body: Vec::new(),
            }),
            Step::Fail(error) => Err(error),
            Step::FailAfter(ms, error) => {
                self.clock.advance(ms);
                Err(error)
            }
        }
    }
}
