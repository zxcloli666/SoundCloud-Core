use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use equix::{EquiXBuilder, RuntimeOption, SolverMemory};

use super::{NONCE_LEN, POW_V, SEED_LEN, challenge, meets_effort};
use crate::proto::Pow;
use crate::random_bytes;

pub struct Ticket {
    pub seed: [u8; SEED_LEN],
    pub effort: u32,
}

pub fn solve(
    ticket: &Ticket,
    auth_pub: &[u8; 32],
    path: &str,
    threads: usize,
    cancel: &AtomicBool,
) -> Option<Pow> {
    let done = AtomicBool::new(false);
    let found = Mutex::new(None);
    thread::scope(|scope| {
        for _ in 0..threads.max(1) {
            scope.spawn(|| work(ticket, auth_pub, path, cancel, &done, &found));
        }
    });
    found.into_inner().ok().flatten()
}

fn work(
    ticket: &Ticket,
    auth_pub: &[u8; 32],
    path: &str,
    cancel: &AtomicBool,
    done: &AtomicBool,
    found: &Mutex<Option<Pow>>,
) {
    let Ok(start) = random_bytes::<NONCE_LEN>() else {
        return;
    };
    let mut counter = u128::from_be_bytes(start);
    let mut builder = EquiXBuilder::new();
    builder.runtime(RuntimeOption::TryCompile);
    let mut memory = SolverMemory::new();
    while !cancel.load(Ordering::Relaxed) && !done.load(Ordering::Relaxed) {
        let nonce = counter.to_be_bytes();
        counter = counter.wrapping_add(1);
        let bytes = challenge(&ticket.seed, auth_pub, path, &nonce, ticket.effort);
        let Ok(equix) = builder.build(&bytes) else {
            continue;
        };
        for solution in equix.solve_with_memory(&mut memory) {
            let solution = solution.to_bytes();
            if !meets_effort(&bytes, &solution, ticket.effort) {
                continue;
            }
            if let Ok(mut slot) = found.lock() {
                slot.get_or_insert(Pow {
                    v: POW_V,
                    seed: ticket.seed.to_vec(),
                    effort: ticket.effort,
                    nonce: nonce.to_vec(),
                    solution: solution.to_vec(),
                });
            }
            done.store(true, Ordering::Relaxed);
            return;
        }
    }
}
