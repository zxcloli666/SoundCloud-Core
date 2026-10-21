use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use sc_chat::{Clock, async_trait};

pub struct TestClock {
    now: AtomicI64,
    slept: Mutex<Vec<u64>>,
    block_from: AtomicI64,
}

impl TestClock {
    pub fn at(now: i64) -> Self {
        Self {
            now: AtomicI64::new(now),
            slept: Mutex::new(Vec::new()),
            block_from: AtomicI64::new(i64::MAX),
        }
    }

    pub fn advance(&self, ms: u64) {
        self.now.fetch_add(ms as i64, Ordering::SeqCst);
    }

    pub fn slept(&self) -> Vec<u64> {
        self.slept.lock().unwrap().clone()
    }

    pub fn block_sleeps_from(&self, ms: u64) {
        self.block_from.store(ms as i64, Ordering::SeqCst);
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }

    async fn sleep(&self, ms: u64) {
        self.slept.lock().unwrap().push(ms);
        if ms as i64 >= self.block_from.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        self.advance(ms);
    }
}
