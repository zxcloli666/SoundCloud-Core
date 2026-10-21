use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::device::DeviceKeys;
use super::message::Ck;
use crate::contracts::{ChatResult, ConvId, DeviceId};

pub const CK_CACHE_SIZE: usize = 1024;

type Slot = (ConvId, u32);

pub struct CkCache {
    capacity: usize,
    slots: Mutex<Slots>,
}

#[derive(Default)]
struct Slots {
    keys: HashMap<Slot, Arc<Ck>>,
    order: VecDeque<Slot>,
}

impl CkCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            slots: Mutex::default(),
        }
    }

    pub fn get(&self, conversation_id: ConvId, epoch: u32) -> Option<Arc<Ck>> {
        self.slots().keys.get(&(conversation_id, epoch)).cloned()
    }

    pub fn insert(&self, conversation_id: ConvId, epoch: u32, ck: Ck) -> Arc<Ck> {
        let ck = Arc::new(ck);
        let mut slots = self.slots();
        if slots
            .keys
            .insert((conversation_id, epoch), ck.clone())
            .is_none()
        {
            slots.order.push_back((conversation_id, epoch));
        }
        while slots.keys.len() > self.capacity {
            let Some(oldest) = slots.order.pop_front() else {
                break;
            };
            slots.keys.remove(&oldest);
        }
        ck
    }

    pub fn open(
        &self,
        keys: &DeviceKeys,
        conversation_id: ConvId,
        epoch: u32,
        device_id: DeviceId,
        sealed: &[u8],
    ) -> ChatResult<Arc<Ck>> {
        let ck = keys.open_ck(conversation_id, epoch, device_id, sealed)?;
        Ok(self.insert(conversation_id, epoch, ck))
    }

    pub fn forget(&self, conversation_id: ConvId) {
        let mut slots = self.slots();
        slots.keys.retain(|(id, _), _| *id != conversation_id);
        slots.order.retain(|(id, _)| *id != conversation_id);
    }

    pub fn clear(&self) {
        let mut slots = self.slots();
        slots.keys.clear();
        slots.order.clear();
    }

    pub fn len(&self) -> usize {
        self.slots().keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn slots(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for CkCache {
    fn default() -> Self {
        Self::new(CK_CACHE_SIZE)
    }
}
