use sc_chat_wire::WireError;
use sc_chat_wire::pow::{POW_V, verify};
use sc_chat_wire::proto::Pow;

const SEED: [u8; 32] = [7; 32];
const AUTH_PUB: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31,
];
const PATH: &str = "/auth/x";
const EFFORT: u32 = 20;
const NONCE: &str = "00000000000000000000000000000002";
const SOLUTION: &str = "5a559561b84137927e497d5f68ada6ec";

fn vector() -> Pow {
    Pow {
        v: POW_V,
        seed: SEED.to_vec(),
        effort: EFFORT,
        nonce: hex::decode(NONCE).unwrap(),
        solution: hex::decode(SOLUTION).unwrap(),
    }
}

#[test]
fn fixed_vector_verifies() {
    assert_eq!(verify(&vector(), &AUTH_PUB, PATH), Ok(()));
}

#[test]
fn fixed_vector_breaks_on_any_change() {
    assert_eq!(verify(&vector(), &AUTH_PUB, "/auth/y"), Err(WireError::Pow));

    let mut other_key = AUTH_PUB;
    other_key[0] ^= 1;
    assert_eq!(verify(&vector(), &other_key, PATH), Err(WireError::Pow));

    let mut effort = vector();
    effort.effort += 1;
    assert_eq!(verify(&effort, &AUTH_PUB, PATH), Err(WireError::Pow));

    let mut flipped = vector();
    flipped.solution[0] ^= 1;
    assert_eq!(verify(&flipped, &AUTH_PUB, PATH), Err(WireError::Pow));

    let mut version = vector();
    version.v = 2;
    assert_eq!(verify(&version, &AUTH_PUB, PATH), Err(WireError::Pow));

    let mut zero = vector();
    zero.effort = 0;
    assert_eq!(verify(&zero, &AUTH_PUB, PATH), Err(WireError::Pow));
}

#[test]
fn wrong_lengths_fail() {
    let mut seed = vector();
    seed.seed.pop();
    assert_eq!(verify(&seed, &AUTH_PUB, PATH), Err(WireError::Pow));

    let mut nonce = vector();
    nonce.nonce.push(0);
    assert_eq!(verify(&nonce, &AUTH_PUB, PATH), Err(WireError::Pow));

    let mut solution = vector();
    solution.solution.pop();
    assert_eq!(verify(&solution, &AUTH_PUB, PATH), Err(WireError::Pow));

    assert_eq!(verify(&vector(), &AUTH_PUB[1..], PATH), Err(WireError::Pow));
    assert_eq!(
        verify(&Pow::default(), &AUTH_PUB, PATH),
        Err(WireError::Pow)
    );
}

#[cfg(feature = "solve")]
mod solved {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use sc_chat_wire::WireError;
    use sc_chat_wire::pow::{Ticket, solve, verify};

    use super::{AUTH_PUB, PATH, SEED};

    fn ticket(effort: u32) -> Ticket {
        Ticket { seed: SEED, effort }
    }

    #[test]
    fn round_trip_at_effort_1_and_20() {
        for effort in [1, 20] {
            let cancel = AtomicBool::new(false);
            let pow = solve(&ticket(effort), &AUTH_PUB, PATH, 2, &cancel).unwrap();
            assert_eq!(pow.effort, effort);
            assert_eq!(verify(&pow, &AUTH_PUB, PATH), Ok(()));
            assert_eq!(verify(&pow, &AUTH_PUB, "/other"), Err(WireError::Pow));
        }
    }

    #[test]
    fn cancel_stops_the_search() {
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(300));
                cancel.store(true, Ordering::Relaxed);
            });
            assert!(solve(&ticket(u32::MAX), &AUTH_PUB, PATH, 2, &cancel).is_none());
        });
    }
}
