use std::cmp::Reverse;
use std::collections::HashSet;

use super::gate::LOCAL_MAX;
use super::local::Local;
use super::{SyncEngine, summary};
use crate::api::wire::ConversationView;
use crate::contracts::{
    ChatError, ChatResult, ConvId, CoreEvent, MessagesEvent, MessagesOrigin, Seq, SyncEvent,
};
use crate::store::SyncState;

pub const LIST_PAGES_MAX: usize = 100;
pub const CATCH_UP_BUDGET: usize = 32;

struct Listed {
    views: Vec<ConversationView>,
    complete: bool,
}

impl<L: Local> SyncEngine<L> {
    pub fn reset_pending(&self) -> bool {
        self.with(|state| state.reset.is_some())
    }

    pub async fn reconcile(&self) -> ChatResult<()> {
        let Some(pending) = self.with(|state| state.reset) else {
            return Ok(());
        };
        let listed = self.server_list().await?;
        let local = self.store.known(LOCAL_MAX)?;
        if listed.complete {
            let ids: HashSet<ConvId> = listed.views.iter().map(|view| view.id).collect();
            let mut gone = Vec::new();
            for (id, _) in &local {
                if !ids.contains(id) && !self.is_room(*id)? {
                    gone.push(*id);
                }
            }
            self.purge(&gone, &[])?;
        }
        if pending.restored {
            let server_last: Vec<(ConvId, Seq)> = listed
                .views
                .iter()
                .filter(|view| local.iter().any(|(id, _)| *id == view.id))
                .map(|view| (view.id, view.last_seq))
                .collect();
            self.store.keep_local_above(&server_last)?;
        }
        let mut conversations = Vec::with_capacity(listed.views.len());
        let mut budget = CATCH_UP_BUDGET;
        for view in &listed.views {
            self.note_view(view);
            let existing = self.store.conversation(view.id)?;
            let mut summary = self.summary_of(view, existing.as_ref()).await?;
            let mut messages = Vec::new();
            if summary.last_seq < view.last_seq && budget > 0 {
                budget -= 1;
                messages = self.catch_up(view.id, summary.last_seq).await?;
                summary::absorb(&mut summary, &messages);
                self.note_delivered(view.id, &messages);
            }
            self.recount(&mut summary)?;
            self.save(std::slice::from_ref(&summary))?;
            if !messages.is_empty() {
                self.events.emit(CoreEvent::Messages(MessagesEvent {
                    conversation_id: view.id,
                    origin: MessagesOrigin::Sync,
                    messages,
                }));
            }
            conversations.push(summary);
        }
        conversations.extend(self.catch_up_rooms(&local).await?);
        let requests = self.with(|state| state.status.requests);
        self.events.emit(CoreEvent::Sync(SyncEvent {
            reset: true,
            conversations,
            marks: Vec::new(),
            requests,
        }));
        self.store.set_sync_state(SyncState {
            pos: pending.pos,
            inc: Some(pending.inc),
        })?;
        self.with(|state| {
            if state.reset == Some(pending) {
                state.reset = None;
            }
        });
        Ok(())
    }

    async fn server_list(&self) -> ChatResult<Listed> {
        let mut views = Vec::new();
        let mut cursor: Option<String> = None;
        let mut complete = false;
        for _ in 0..LIST_PAGES_MAX {
            let page = self.api.conversations(cursor.as_deref()).await?;
            views.extend(page.items);
            match page.next_cursor {
                None => {
                    complete = true;
                    break;
                }
                Some(next) if cursor.as_ref() == Some(&next) => return Err(ChatError::Protocol),
                next => cursor = next,
            }
        }
        views.sort_by_key(|view| Reverse(view.last_at));
        Ok(Listed { views, complete })
    }
}
