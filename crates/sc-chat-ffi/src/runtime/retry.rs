use std::time::Duration;

const RETRY_MIN_MS: u64 = 2_000;
const RETRY_MAX_MS: u64 = 60_000;

pub fn backoff(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(5);
    let pause_ms = (RETRY_MIN_MS << exponent).min(RETRY_MAX_MS);
    Duration::from_millis(spread(pause_ms))
}

pub fn busy(retry_ms: u64) -> Duration {
    Duration::from_millis(spread(retry_ms.max(RETRY_MIN_MS)))
}

pub fn clock(failures: u32) -> Duration {
    if failures <= 1 {
        Duration::ZERO
    } else {
        backoff(failures)
    }
}

pub fn jitter(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    let roll = sc_chat::sc_chat_wire::random_bytes::<8>()
        .map(u64::from_le_bytes)
        .unwrap_or(0);
    roll % (max_ms + 1)
}

fn spread(pause_ms: u64) -> u64 {
    pause_ms - pause_ms / 5 + jitter(pause_ms * 2 / 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_up_to_a_minute_within_a_fifth_either_way() {
        for (failures, base) in [
            (1, 2_000),
            (2, 4_000),
            (4, 16_000),
            (6, 60_000),
            (40, 60_000),
        ] {
            let pause = backoff(failures).as_millis() as u64;
            assert!(
                pause >= base * 4 / 5 && pause <= base * 6 / 5,
                "{failures}: {pause}"
            );
        }
    }

    #[test]
    fn a_busy_answer_waits_at_least_the_floor() {
        assert!(busy(0).as_millis() >= 1_600);
        assert!(busy(30_000).as_millis() >= 24_000);
    }

    #[test]
    fn the_first_clock_miss_retries_at_once() {
        assert_eq!(clock(1), Duration::ZERO);
        assert!(clock(2) > Duration::ZERO);
    }
}
