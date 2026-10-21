pub mod client;
pub mod http;
pub mod people;
pub mod transport;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use http::{Reply, Request};

pub use client::Client;
pub use people::Person;

const PLAIN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Stack {
    pub origin: String,
    pub relay: String,
    psql: Vec<String>,
    redis: Vec<String>,
    admin_token: String,
    ai_key: String,
    log: PathBuf,
}

fn read_trimmed(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|_| panic!("{} is readable", path.display()))
        .trim()
        .to_owned()
}

fn words(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_owned).collect()
}

fn run(command: &[String], args: &[&str]) -> String {
    let output = Command::new(&command[0])
        .args(&command[1..])
        .args(args)
        .output()
        .expect("the stack command runs");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

impl Stack {
    pub fn from_env() -> Self {
        let need = |name: &str| {
            std::env::var(name)
                .unwrap_or_else(|_| panic!("{name} is not set, see tests/integration/README.md"))
        };
        let token_file = PathBuf::from(need("CHAT_IT_ADMIN_TOKEN_FILE"));
        assert!(
            std::env::var(sc_chat::channel::DEV_OCK_ENV).is_ok(),
            "{} is not set",
            sc_chat::channel::DEV_OCK_ENV
        );
        Self {
            origin: need("CHAT_IT_ORIGIN"),
            relay: need("CHAT_IT_RELAY"),
            psql: words(&need("CHAT_IT_PSQL")),
            redis: words(&need("CHAT_IT_REDIS_RT")),
            admin_token: read_trimmed(&token_file),
            ai_key: read_trimmed(&token_file.with_file_name("mod-ai-key")),
            log: PathBuf::from(need("CHAT_IT_LOG")),
        }
    }

    pub fn ai_key(&self) -> &str {
        &self.ai_key
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(&self.log).expect("the chat-api log is readable")
    }

    pub fn sql(&self, statement: &str) -> String {
        run(
            &self.psql,
            &["-v", "ON_ERROR_STOP=1", "-qAt", "-c", statement],
        )
    }

    pub fn redis(&self, args: &[&str]) -> String {
        run(&self.redis, args)
    }

    pub fn count(&self, statement: &str) -> i64 {
        self.sql(statement)
            .parse()
            .expect("the query answers a number")
    }

    pub async fn admin(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        let mut headers = vec![
            ("x-chat-admin-token", self.admin_token.clone()),
            ("x-chat-admin-user", "integration".to_owned()),
        ];
        if body.is_some() {
            headers.push(("content-type", "application/json".to_owned()));
        }
        self.plain(method, path, headers, body).await
    }

    pub async fn as_session(
        &self,
        session_id: uuid::Uuid,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Reply {
        let headers = vec![
            ("x-session-id", session_id.to_string()),
            ("content-type", "application/json".to_owned()),
        ];
        self.plain(method, path, headers, body).await
    }

    async fn plain(
        &self,
        method: &str,
        path: &str,
        headers: Vec<(&str, String)>,
        body: Option<Value>,
    ) -> Reply {
        let request = Request {
            method,
            path,
            headers,
            body: body
                .map(|body| body.to_string().into_bytes())
                .unwrap_or_default(),
            timeout: PLAIN_TIMEOUT,
        };
        http::send(&self.origin, request)
            .await
            .unwrap_or_else(|error| panic!("{method} {path}: {error}"))
    }
}

pub const SOON: Duration = Duration::from_secs(10);

pub async fn eventually<F, Fut>(what: &str, check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    within(what, SOON, check).await;
}

pub async fn within<F, Fut>(what: &str, limit: Duration, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("{what} did not happen within {} s", limit.as_secs());
}
