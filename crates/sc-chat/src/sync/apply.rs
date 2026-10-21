use std::collections::HashMap;

use super::local::Local;
use super::state::PendingReset;
use super::{SyncEngine, SyncOutcome, summary};
use crate::api::wire::{SyncAnswer, SyncConversation};
use crate::contracts::{
    ChatResult, ConvId, ConversationMarks, ConversationSummary, CoreEvent, GoneEvent, GoneMessage,
    Message, MessagesEvent, MessagesOrigin, PeerMark, Seq, SyncEvent, TypingEvent,
};
use crate::store::SyncState;

struct Applied {
    summary: ConversationSummary,
    marks: Vec<PeerMark>,
}

impl<L: Local> SyncEngine<L> {
    pub(super) async fn apply(
        &self,
        answer: SyncAnswer,
        known: &HashMap<ConvId, Seq>,
    ) -> ChatResult<SyncOutcome> {
        let stored = self.store.sync_state()?;
        let mut changed = self.purge(&answer.gone, &answer.gone_msgs)?;
        if !answer.devices.is_empty() {
            self.devices_seen(&answer.devices).await?;
        }
        let requests_changed = self.note_requests(answer.requests_count);
        if answer.reset || stored.inc != Some(answer.inc) {
            let restored = stored.inc.is_some_and(|inc| inc != answer.inc);
            let pending = PendingReset {
                pos: answer.pos,
                inc: answer.inc,
                restored,
            };
            self.with(|state| state.reset = Some(pending));
            return Ok(SyncOutcome::Reset);
        }
        let mut event = SyncEvent {
            reset: false,
            conversations: Vec::new(),
            marks: Vec::new(),
            requests: answer.requests_count,
        };
        let mut more = false;
        for entry in &answer.convs {
            let sent = known.get(&entry.id).copied();
            more |= entry.more && sent.is_some();
            let Some(applied) = self.apply_entry(entry, sent).await? else {
                continue;
            };
            if !applied.marks.is_empty() {
                event.marks.push(ConversationMarks {
                    conversation_id: entry.id,
                    marks: applied.marks,
                });
            }
            event.conversations.push(applied.summary);
        }
        changed |= !event.conversations.is_empty();
        if !event.conversations.is_empty() || requests_changed {
            self.events.emit(CoreEvent::Sync(event));
        }
        self.store.set_sync_state(SyncState {
            pos: answer.pos,
            inc: Some(answer.inc),
        })?;
        self.with(|state| state.more = more);
        Ok(if changed {
            SyncOutcome::Changed
        } else {
            SyncOutcome::Idle
        })
    }

    async fn apply_entry(
        &self,
        entry: &SyncConversation,
        sent: Option<Seq>,
    ) -> ChatResult<Option<Applied>> {
        if let Ok(epoch) = u32::try_from(entry.key_epoch) {
            self.with(|state| state.note_epoch(entry.id, epoch));
        }
        let existing = self.store.conversation(entry.id)?;
        let local_last = existing.as_ref().map(|summary| summary.last_seq);
        let behind = sent.is_none() && entry.last_seq > local_last.unwrap_or_default();
        let Some(mut summary) = self.summary_for(entry, existing, behind).await? else {
            return Ok(None);
        };
        let mut messages = self.take_rows(entry.id, &entry.messages).await?;
        if behind && entry.messages.is_empty() {
            messages.extend(self.catch_up(entry.id, summary.last_seq).await?);
        }
        messages.sort_by_key(|message| message.seq);
        summary::absorb(&mut summary, &messages);
        let marks = self.take_marks(entry, &mut summary)?;
        self.save(std::slice::from_ref(&summary))?;
        self.note_delivered(entry.id, &messages);
        if self.focus() == Some(entry.id) {
            self.events.emit(CoreEvent::Typing(TypingEvent {
                conversation_id: entry.id,
                users: entry.typing.clone(),
            }));
        }
        if !messages.is_empty() {
            self.events.emit(CoreEvent::Messages(MessagesEvent {
                conversation_id: entry.id,
                origin: MessagesOrigin::Sync,
                messages,
            }));
        }
        Ok(Some(Applied { summary, marks }))
    }

    fn take_marks(
        &self,
        entry: &SyncConversation,
        summary: &mut ConversationSummary,
    ) -> ChatResult<Vec<PeerMark>> {
        let marks: Vec<PeerMark> = entry
            .marks
            .iter()
            .map(|mark| PeerMark {
                user_id: mark.user_id.clone(),
                read_seq: mark.read_seq,
                delivered_seq: mark.delivered_seq,
            })
            .collect();
        if marks.is_empty() {
            return Ok(marks);
        }
        self.store.put_marks(entry.id, &marks)?;
        let me = self.with(|state| state.user.clone());
        let mine = marks
            .iter()
            .find(|mark| me.as_ref() == Some(&mark.user_id))
            .map(|mark| mark.read_seq);
        if let Some(read) = mine.filter(|read| *read > summary.last_read_seq) {
            summary.last_read_seq = read;
            self.recount(summary)?;
        }
        Ok(marks)
    }

    pub(super) fn note_delivered(&self, conversation_id: ConvId, messages: &[Message]) {
        let me = self.with(|state| state.user.clone());
        let newest = messages
            .iter()
            .filter(|message| me.as_ref() != Some(&message.sender))
            .filter_map(|message| message.seq)
            .max();
        if let Some(seq) = newest {
            self.with(|state| state.marks.delivered(conversation_id, seq));
        }
    }

    pub(super) fn purge(&self, gone: &[ConvId], gone_msgs: &[(ConvId, Seq)]) -> ChatResult<bool> {
        if gone.is_empty() && gone_msgs.is_empty() {
            return Ok(false);
        }
        let messages: Vec<GoneMessage> = gone_msgs
            .iter()
            .filter(|(id, _)| !gone.contains(id))
            .map(|(id, seq)| GoneMessage {
                conversation_id: *id,
                seq: *seq,
            })
            .collect();
        self.store.purge_conversations(gone)?;
        self.store.purge_messages(&messages)?;
        self.untally(gone);
        self.with(|state| {
            for id in gone {
                state.marks.forget(*id);
                state.epochs.remove(id);
            }
        });
        let mut touched: Vec<ConvId> = messages.iter().map(|gone| gone.conversation_id).collect();
        touched.sort();
        touched.dedup();
        self.events.emit(CoreEvent::Gone(GoneEvent {
            conversations: gone.to_vec(),
            messages,
        }));
        for id in touched {
            self.refresh_preview(id)?;
        }
        Ok(true)
    }
}
