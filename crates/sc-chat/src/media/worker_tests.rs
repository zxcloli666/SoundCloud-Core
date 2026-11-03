use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use super::{MediaError, MediaWorker};

#[test]
fn a_slow_job_times_out_and_keeps_its_slot() {
    let worker = MediaWorker::new(1, Duration::from_millis(60));
    let slow = worker.run(|| {
        thread::sleep(Duration::from_millis(400));
        Ok(())
    });
    assert_eq!(slow, Err(MediaError::TimedOut));
    assert_eq!(worker.run(|| Ok(1)), Err(MediaError::Busy));
    thread::sleep(Duration::from_millis(500));
    assert_eq!(worker.run(|| Ok(1)), Ok(1));
}

#[test]
fn a_panicking_job_is_a_typed_error_and_frees_the_slot() {
    let worker = MediaWorker::new(1, Duration::from_secs(5));
    let crashed: Result<(), MediaError> = worker.run(|| panic!("decoder bug"));
    assert_eq!(crashed, Err(MediaError::Malformed));
    assert_eq!(worker.run(|| Ok(2)), Ok(2));
}

#[test]
fn no_more_jobs_run_at_once_than_the_slots() {
    let worker = MediaWorker::new(2, Duration::from_secs(10));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let start = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let (worker, running, peak, start) =
                (worker.clone(), running.clone(), peak.clone(), start.clone());
            thread::spawn(move || {
                start.wait();
                worker
                    .run(move || {
                        let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(30));
                        running.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .unwrap();
            })
        })
        .collect();
    threads
        .into_iter()
        .for_each(|handle| handle.join().unwrap());
    assert_eq!(peak.load(Ordering::SeqCst), 2);
}
