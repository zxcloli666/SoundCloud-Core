use super::local::Local;
use super::meta_read::MetaRead;
use super::{SyncEngine, fatal, transient};
use crate::api::wire::MessageView;
use crate::contracts::{
    ChatError, ChatResult, ConvId, ConversationKind, ConversationSummary, MessageClass, Millis,
};

pub const ROOM_META_RETRY_MS: Millis = 30_000;

impl<L: Local> SyncEngine<L> {
    pub(super) fn note_room_meta(&self, id: ConvId, views: &[MessageView]) -> ChatResult<()> {
        let carries_meta = views
            .iter()
            .any(|view| view.class == MessageClass::Meta.as_str());
        if carries_meta && self.is_room(id)? {
            let now = self.now();
            self.with(|state| state.room_meta_due.insert(id, now));
        }
        Ok(())
    }

    pub(super) async fn room_meta(
        &self,
        summary: &mut ConversationSummary,
        refresh: bool,
    ) -> ChatResult<()> {
        if summary.kind != ConversationKind::Room {
            return Ok(());
        }
        let now = self.now();
        let due = self.with(|state| {
            let due = state
                .room_meta_due
                .get(&summary.id)
                .is_some_and(|at| *at <= now);
            due || !state.room_meta_read.contains(&summary.id)
        });
        if refresh || due {
            self.refresh_room_meta(summary.id).await?;
        }
        summary.avatar_media_id = None;
        Ok(())
    }

    async fn refresh_room_meta(&self, id: ConvId) -> ChatResult<()> {
        self.with(|state| {
            state.room_meta_due.remove(&id);
            state.room_meta_read.insert(id);
        });
        let view = match self.api.conversation(id).await {
            Ok(view) if view.id == id => view,
            Ok(_) => return self.room_meta_missed(id, ChatError::Protocol),
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.note_view(&view);
        let read = match self.read_meta(&view).await {
            Ok(read) => read,
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.keep_room_meta(id, read)
    }

    pub(super) fn keep_room_meta(&self, id: ConvId, read: MetaRead) -> ChatResult<()> {
        self.with(|state| {
            state.room_meta_read.insert(id);
            match &read {
                MetaRead::Locked(Some(epoch)) => state.room_meta_locked.insert(id, *epoch),
                _ => state.room_meta_locked.remove(&id),
            }
        });
        self.store.put_room_meta(id, read.opened().as_ref())
    }

    fn room_meta_missed(&self, id: ConvId, error: ChatError) -> ChatResult<()> {
        if fatal(&error) {
            return Err(error);
        }
        if transient(&error) {
            let wait = match error {
                ChatError::Busy { retry_ms } => i64::try_from(retry_ms).unwrap_or(Millis::MAX),
                _ => 0,
            };
            let at = self.now().saturating_add(wait.max(ROOM_META_RETRY_MS));
            self.with(|state| {
                state.room_meta_due.entry(id).or_insert(at);
            });
        }
        Ok(())
    }
}
