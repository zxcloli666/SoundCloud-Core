use std::collections::HashMap;

use crate::api::wire::MARKS_MAX;
use crate::contracts::{ConvId, Seq};

#[derive(Debug, Default)]
pub struct Marks {
    read: HashMap<ConvId, Seq>,
    delivered: HashMap<ConvId, Seq>,
    sent_read: HashMap<ConvId, Seq>,
    sent_delivered: HashMap<ConvId, Seq>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkBatch {
    pub read: Vec<(ConvId, Seq)>,
    pub delivered: Vec<(ConvId, Seq)>,
}

impl Marks {
    pub fn read(&mut self, conversation_id: ConvId, seq: Seq) -> bool {
        raise(&mut self.read, &self.sent_read, conversation_id, seq)
    }

    pub fn delivered(&mut self, conversation_id: ConvId, seq: Seq) -> bool {
        raise(
            &mut self.delivered,
            &self.sent_delivered,
            conversation_id,
            seq,
        )
    }

    pub fn batch(&self) -> MarkBatch {
        MarkBatch {
            read: capped(&self.read),
            delivered: capped(&self.delivered),
        }
    }

    pub fn commit(&mut self, batch: &MarkBatch) {
        settle(&mut self.read, &mut self.sent_read, &batch.read);
        settle(
            &mut self.delivered,
            &mut self.sent_delivered,
            &batch.delivered,
        );
    }

    pub fn forget(&mut self, conversation_id: ConvId) {
        self.read.remove(&conversation_id);
        self.delivered.remove(&conversation_id);
        self.sent_read.remove(&conversation_id);
        self.sent_delivered.remove(&conversation_id);
    }
}

fn raise(
    pending: &mut HashMap<ConvId, Seq>,
    sent: &HashMap<ConvId, Seq>,
    conversation_id: ConvId,
    seq: Seq,
) -> bool {
    let floor = pending
        .get(&conversation_id)
        .into_iter()
        .chain(sent.get(&conversation_id))
        .copied()
        .max();
    if floor.is_some_and(|floor| seq <= floor) {
        return false;
    }
    pending.insert(conversation_id, seq);
    true
}

fn capped(pending: &HashMap<ConvId, Seq>) -> Vec<(ConvId, Seq)> {
    let mut marks: Vec<(ConvId, Seq)> = pending.iter().map(|(id, seq)| (*id, *seq)).collect();
    marks.sort();
    marks.truncate(MARKS_MAX);
    marks
}

fn settle(
    pending: &mut HashMap<ConvId, Seq>,
    sent: &mut HashMap<ConvId, Seq>,
    batch: &[(ConvId, Seq)],
) {
    for (id, seq) in batch {
        if pending.get(id).is_some_and(|queued| queued <= seq) {
            pending.remove(id);
        }
        let acked = sent.entry(*id).or_insert(*seq);
        *acked = (*acked).max(*seq);
    }
}
