use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::MediaError;

pub const DECODE_THREADS: usize = 2;
pub const DECODE_TIMEOUT: Duration = Duration::from_secs(8);

struct Slots {
    free: Mutex<usize>,
    freed: Condvar,
}

struct Permit(Arc<Slots>);

impl Drop for Permit {
    fn drop(&mut self) {
        let mut free = self.0.free.lock().unwrap_or_else(|e| e.into_inner());
        *free += 1;
        self.0.freed.notify_one();
    }
}

#[derive(Clone)]
pub struct MediaWorker {
    slots: Arc<Slots>,
    timeout: Duration,
}

impl Default for MediaWorker {
    fn default() -> Self {
        Self::new(DECODE_THREADS, DECODE_TIMEOUT)
    }
}

impl MediaWorker {
    pub fn new(threads: usize, timeout: Duration) -> Self {
        Self {
            slots: Arc::new(Slots {
                free: Mutex::new(threads.max(1)),
                freed: Condvar::new(),
            }),
            timeout,
        }
    }

    pub(super) fn run<T, F>(&self, job: F) -> Result<T, MediaError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, MediaError> + Send + 'static,
    {
        let permit = self.acquire()?;
        let (done, result) = mpsc::channel();
        thread::Builder::new()
            .name("chat-media-decode".into())
            .spawn(move || {
                let _permit = permit;
                let _ = done.send(job());
            })
            .map_err(|_| MediaError::Busy)?;
        match result.recv_timeout(self.timeout) {
            Ok(outcome) => outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(MediaError::TimedOut),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(MediaError::Malformed),
        }
    }

    fn acquire(&self) -> Result<Permit, MediaError> {
        let deadline = Instant::now() + self.timeout;
        let mut free = self.slots.free.lock().map_err(|_| MediaError::Busy)?;
        while *free == 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(MediaError::Busy);
            }
            free = self
                .slots
                .freed
                .wait_timeout(free, left)
                .map_err(|_| MediaError::Busy)?
                .0;
        }
        *free -= 1;
        Ok(Permit(Arc::clone(&self.slots)))
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod worker_tests;
