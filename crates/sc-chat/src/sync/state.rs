use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use super::marks::Marks;
use super::tally::Tally;
use super::tiers::Tiers;
use crate::contracts::{ChatStatus, ConvId, DeviceId, Millis, UserId, VISIBLE_HOLD_MAX_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingReset {
    pub pos: u64,
    pub inc: Uuid,
    pub restored: bool,
}

#[derive(Debug)]
pub struct State {
    pub tiers: Tiers,
    pub marks: Marks,
    pub focus: Option<ConvId>,
    pub user: Option<UserId>,
    pub hold_cap: u64,
    pub more: bool,
    pub errors: u32,
    pub epochs: HashMap<ConvId, u32>,
    pub locked_keys: HashSet<(ConvId, u32)>,
    pub alerted: HashSet<DeviceId>,
    pub device_created_at: Option<Millis>,
    pub reset: Option<PendingReset>,
    pub tally: Tally,
    pub status: ChatStatus,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tiers: Tiers::default(),
            marks: Marks::default(),
            focus: None,
            user: None,
            hold_cap: VISIBLE_HOLD_MAX_MS,
            more: false,
            errors: 0,
            epochs: HashMap::new(),
            locked_keys: HashSet::new(),
            alerted: HashSet::new(),
            device_created_at: None,
            reset: None,
            tally: Tally::default(),
            status: ChatStatus::off(),
        }
    }
}

impl State {
    pub fn note_epoch(&mut self, conversation_id: ConvId, epoch: u32) {
        let newest = self.epochs.entry(conversation_id).or_insert(epoch);
        *newest = (*newest).max(epoch);
    }
}
