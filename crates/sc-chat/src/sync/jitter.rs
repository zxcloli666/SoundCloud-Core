pub fn upto(max_ms: u64) -> u64 {
    match max_ms {
        0 => 0,
        max => random() % (max + 1),
    }
}

pub fn around(base_ms: u64) -> u64 {
    let half = base_ms / 2;
    half + upto(base_ms - half)
}

pub fn doubling(attempt: u32, start_ms: u64, max_ms: u64) -> u64 {
    let shift = attempt.saturating_sub(1).min(16);
    start_ms.saturating_mul(1 << shift).min(max_ms)
}

fn random() -> u64 {
    sc_chat_wire::random_bytes::<8>()
        .map(u64::from_le_bytes)
        .unwrap_or_default()
}
