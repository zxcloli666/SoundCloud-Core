use super::conversations::MetaRead;
use super::local::Local;
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
            state
                .room_meta_due
                .get(&summary.id)
                .is_some_and(|at| *at <= now)
        });
        if refresh || due {
            self.refresh_room_meta(summary.id).await?;
        }
        summary.avatar_media_id = self.store.meta_image(summary.id)?;
        Ok(())
    }

    async fn refresh_room_meta(&self, id: ConvId) -> ChatResult<()> {
        self.with(|state| state.room_meta_due.remove(&id));
        let view = match self.api.conversation(id).await {
            Ok(view) => view,
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.note_view(&view);
        let meta = match self.read_meta(&view).await {
            Ok(MetaRead::Open(meta)) => Some(*meta),
            Ok(MetaRead::Absent | MetaRead::Locked) => None,
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.store.put_room_meta(id, meta.as_ref())
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
