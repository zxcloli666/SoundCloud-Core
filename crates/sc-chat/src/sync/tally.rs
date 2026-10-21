use std::collections::HashMap;

use super::SyncEngine;
use super::gate::LOCAL_MAX;
use super::local::Local;
use crate::contracts::{
    ChatResult, ConvId, ConversationKind, ConversationSummary, MemberState, Millis, NotifyLevel,
};

#[derive(Debug, Default)]
pub struct Tally {
    chats: HashMap<ConvId, Counted>,
}

#[derive(Debug, Clone, Copy)]
struct Counted {
    unread: bool,
    mentioned: bool,
}

impl Tally {
    pub fn note(&mut self, summary: &ConversationSummary, now: Millis) {
        let listed = summary.state == MemberState::Active && summary.notify != NotifyLevel::None;
        let loud = listed
            && summary.notify == NotifyLevel::All
            && summary.kind != ConversationKind::Room
            && summary.muted_until.is_none_or(|until| until <= now);
        let counted = Counted {
            unread: loud && summary.unread > 0,
            mentioned: listed && summary.mentions > 0,
        };
        match counted.unread || counted.mentioned {
            true => self.chats.insert(summary.id, counted),
            false => self.chats.remove(&summary.id),
        };
    }

    pub fn forget(&mut self, id: ConvId) {
        self.chats.remove(&id);
    }

    pub fn clear(&mut self) {
        self.chats.clear();
    }

    pub fn unread(&self) -> u32 {
        count(self.chats.values().filter(|chat| chat.unread))
    }

    pub fn mentions(&self) -> u32 {
        count(self.chats.values().filter(|chat| chat.mentioned))
    }
}

fn count<'a>(chats: impl Iterator<Item = &'a Counted>) -> u32 {
    u32::try_from(chats.count()).unwrap_or(u32::MAX)
}

impl<L: Local> SyncEngine<L> {
    pub fn load_tally(&self) -> ChatResult<()> {
        let now = self.now();
        self.with(|state| state.tally.clear());
        for (id, _) in self.store.known(LOCAL_MAX)? {
            if let Some(summary) = self.store.conversation(id)? {
                self.with(|state| state.tally.note(&summary, now));
            }
        }
        self.publish_tally();
        Ok(())
    }

    pub(super) fn save(&self, summaries: &[ConversationSummary]) -> ChatResult<()> {
        self.store.put_conversations(summaries)?;
        let now = self.now();
        self.with(|state| {
            for summary in summaries {
                state.tally.note(summary, now);
            }
        });
        self.publish_tally();
        Ok(())
    }

    pub(super) fn untally(&self, ids: &[ConvId]) {
        self.with(|state| {
            for id in ids {
                state.tally.forget(*id);
            }
        });
        self.publish_tally();
    }

    fn publish_tally(&self) {
        let (unread, mentions) = self.with(|state| (state.tally.unread(), state.tally.mentions()));
        self.update_status(|status| {
            status.unread = unread;
            status.mentions = mentions;
        });
    }
}
