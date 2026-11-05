use std::sync::atomic::AtomicBool;
use std::thread::available_parallelism;
use std::time::Instant;

use equix::{EquiXBuilder, RuntimeOption};
use sc_chat_wire::pow::{Ticket, challenge, solve, verify};

const SEED: [u8; 32] = [7; 32];
const AUTH_PUB: [u8; 32] = [9; 32];
const PATH: &str = "/auth/x";
const NONCES: u32 = 200;

fn main() {
    let cores = available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!(
        "os={} arch={} cores={cores}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for (name, option) in [
        ("compiled", RuntimeOption::CompileOnly),
        ("interpreted", RuntimeOption::InterpretOnly),
    ] {
        let mut builder = EquiXBuilder::new();
        builder.runtime(option);
        let mut built = 0u32;
        let mut solutions = 0usize;
        let started = Instant::now();
        for n in 0..NONCES {
            let bytes = challenge(&SEED, &AUTH_PUB, PATH, &u128::from(n).to_be_bytes(), 1);
            match builder.build(&bytes) {
                Ok(equix) => {
                    built += 1;
                    solutions += equix.solve().len();
                }
                Err(error) => {
                    println!("{name}: build failed: {error}");
                    break;
                }
            }
        }
        if built == 0 {
            continue;
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0 / f64::from(built);
        println!(
            "{name}: {ms:.2} ms/nonce, {:.2} solutions/nonce ({built} nonces)",
            solutions as f64 / f64::from(built)
        );
    }
    let cancel = AtomicBool::new(false);
    let one = solve(
        &Ticket {
            seed: SEED,
            effort: 1,
        },
        &AUTH_PUB,
        PATH,
        1,
        &cancel,
    )
    .expect("solved");
    let started = Instant::now();
    let rounds = 200;
    for _ in 0..rounds {
        verify(&one, &AUTH_PUB, PATH).expect("verifies");
    }
    println!(
        "verify (interpreted): {:.0} us",
        started.elapsed().as_secs_f64() * 1e6 / f64::from(rounds)
    );
    for effort in [100, 1000] {
        let mut walls = Vec::new();
        for round in 0..5u8 {
            let ticket = Ticket {
                seed: [round; 32],
                effort,
            };
            let started = Instant::now();
            solve(&ticket, &AUTH_PUB, PATH, cores, &cancel).expect("solved");
            walls.push(started.elapsed().as_secs_f64());
        }
        walls.sort_by(f64::total_cmp);
        println!(
            "solve effort {effort}, {cores} threads: median {:.2} s, min {:.2} s, max {:.2} s (5 runs)",
            walls[2], walls[0], walls[4]
        );
    }
}
