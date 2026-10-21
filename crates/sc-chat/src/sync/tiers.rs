use std::collections::HashSet;

use super::jitter;
use crate::contracts::{Millis, REDUCED_HOLD_MS, Tier};

pub const MID_HOLD_MIN_MS: u64 = 1_000;
pub const DROPS_PER_STEP: u8 = 2;
pub const BACKOFF_START_MS: u64 = 1_000;
pub const BACKOFF_MAX_MS: u64 = 15_000;
pub const PREFERRED_RETRY_MS: Millis = 10 * 60_000;

#[derive(Debug, Default)]
pub struct Tiers {
    current: Option<Tier>,
    left_first_at: Option<Millis>,
    failed_rounds: u32,
    drops: u8,
    reduced: HashSet<Tier>,
}

impl Tiers {
    pub fn pick(&mut self, order: &[Tier], now: Millis) -> Option<Tier> {
        let first = *order.first()?;
        let stale = self
            .left_first_at
            .is_some_and(|left| now - left >= PREFERRED_RETRY_MS);
        let tier = match self.current.filter(|tier| order.contains(tier)) {
            Some(tier) if !stale => tier,
            _ => {
                self.left_first_at = None;
                first
            }
        };
        self.current = Some(tier);
        Some(tier)
    }

    pub fn hold(&self, tier: Tier) -> u64 {
        if self.reduced.contains(&tier) {
            REDUCED_HOLD_MS.min(tier.hold_max_ms())
        } else {
            tier.hold_max_ms()
        }
    }

    pub fn answered(&mut self) {
        self.failed_rounds = 0;
    }

    pub fn held(&mut self) {
        self.drops = 0;
    }

    pub fn dropped(&mut self, tier: Tier, order: &[Tier], now: Millis) -> u64 {
        self.drops += 1;
        if self.drops < DROPS_PER_STEP {
            return 0;
        }
        if self.reduced.insert(tier) {
            self.drops = 0;
            return 0;
        }
        self.unreachable(order, now)
    }

    pub fn unreachable(&mut self, order: &[Tier], now: Millis) -> u64 {
        self.drops = 0;
        let at = self
            .current
            .and_then(|tier| order.iter().position(|t| *t == tier))
            .unwrap_or_default();
        if let Some(next) = order.get(at + 1) {
            self.current = Some(*next);
            self.left_first_at.get_or_insert(now);
            return 0;
        }
        self.current = order.first().copied();
        self.left_first_at = None;
        self.failed_rounds += 1;
        jitter::around(jitter::doubling(
            self.failed_rounds,
            BACKOFF_START_MS,
            BACKOFF_MAX_MS,
        ))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
