use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sc_chat::{Clock, Millis};

pub struct SystemClock;

#[async_trait]
impl Clock for SystemClock {
    fn now_ms(&self) -> Millis {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| Millis::try_from(since.as_millis()).unwrap_or(Millis::MAX))
            .unwrap_or_default()
    }

    async fn sleep(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}
