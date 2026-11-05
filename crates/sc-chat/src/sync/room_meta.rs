use super::local::Local;
use super::meta_read::MetaRead;
use super::{SyncEngine, fatal, jitter, transient};
use crate::api::wire::MessageView;
use crate::contracts::{
    ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta, ConversationSummary,
    MessageClass, Millis,
};

pub const ROOM_META_RETRY_MS: Millis = 30_000;
pub const ROOM_META_RETRY_MAX_MS: Millis = 600_000;
pub const ROOM_META_ATTEMPTS_MAX: u32 = 5;
pub const ROOM_META_START_MAX: usize = 32;
pub const ROOM_META_TICK_MAX: usize = 2;

impl<L: Local> SyncEngine<L> {
    pub fn queue_quiet_rooms(&self) -> ChatResult<()> {
        let rooms = self.store.quiet_rooms(ROOM_META_START_MAX)?;
        let now = self.now();
        self.with(|state| {
            state.room_meta_misses.clear();
            for id in rooms {
                if !state.room_meta_read.contains(&id) {
                    state.room_meta_due.entry(id).or_insert(now);
                }
            }
        });
        Ok(())
    }

    pub async fn read_room_metas(&self) -> ChatResult<usize> {
        let now = self.now();
        let due = self.with(|state| {
            let mut due: Vec<(Millis, ConvId)> = state
                .room_meta_due
                .iter()
                .filter(|(_, at)| **at <= now)
                .map(|(id, at)| (*at, *id))
                .collect();
            due.sort_unstable();
            due.truncate(ROOM_META_TICK_MAX);
            due
        });
        let mut tried = 0;
        for (_, id) in due {
            if !self.is_room(id)? {
                self.with(|state| state.room_meta_due.remove(&id));
                continue;
            }
            tried += 1;
            if self.refresh_room_meta(id).await?
                && let Some(summary) = self.store.conversation(id)?
            {
                self.emit_summaries(vec![summary]);
            }
        }
        Ok(tried)
    }

    pub(super) fn note_room_meta(&self, id: ConvId, views: &[MessageView]) -> ChatResult<()> {
        let carries_meta = views
            .iter()
            .any(|view| view.class == MessageClass::Meta.as_str());
        if carries_meta && self.is_room(id)? {
            let now = self.now();
            self.with(|state| {
                state.room_meta_misses.remove(&id);
                state.room_meta_due.insert(id, now);
            });
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

    async fn refresh_room_meta(&self, id: ConvId) -> ChatResult<bool> {
        let fetched = self.fetch_room_meta(id).await;
        self.with(|state| {
            state.room_meta_due.remove(&id);
            state.room_meta_read.insert(id);
        });
        match fetched {
            Ok(read) => {
                self.with(|state| state.room_meta_misses.remove(&id));
                self.keep_room_meta(id, read)?;
                Ok(true)
            }
            Err(error) => {
                self.room_meta_missed(id, error)?;
                Ok(false)
            }
        }
    }

    async fn fetch_room_meta(&self, id: ConvId) -> ChatResult<MetaRead> {
        let view = self.api.conversation(id).await?;
        if view.id != id {
            return Err(ChatError::Protocol);
        }
        self.note_view(&view);
        self.read_meta(&view).await
    }

    pub(super) fn keep_room_meta(&self, id: ConvId, read: MetaRead) -> ChatResult<()> {
        self.with(|state| {
            state.room_meta_read.insert(id);
            match &read {
                MetaRead::Locked(Some(epoch)) => state.room_meta_locked.insert(id, *epoch),
                _ => state.room_meta_locked.remove(&id),
            }
        });
        let known = match read {
            MetaRead::Open(meta) => Some(*meta),
            MetaRead::Absent | MetaRead::Locked(None) => Some(ConversationMeta::default()),
            MetaRead::Locked(Some(_)) => None,
        };
        self.store.put_room_meta(id, known.as_ref())
    }

    fn room_meta_missed(&self, id: ConvId, error: ChatError) -> ChatResult<()> {
        if fatal(&error) {
            return Err(error);
        }
        if !transient(&error) {
            return Ok(());
        }
        let asked = match error {
            ChatError::Busy { retry_ms } => i64::try_from(retry_ms).unwrap_or(Millis::MAX),
            _ => 0,
        };
        let now = self.now();
        self.with(|state| {
            let misses = state.room_meta_misses.entry(id).or_insert(0);
            *misses += 1;
            if *misses >= ROOM_META_ATTEMPTS_MAX {
                return;
            }
            let backoff = jitter::around(jitter::doubling(
                *misses,
                ROOM_META_RETRY_MS.unsigned_abs(),
                ROOM_META_RETRY_MAX_MS.unsigned_abs(),
            ));
            let wait = asked.max(i64::try_from(backoff).unwrap_or(Millis::MAX));
            state.room_meta_due.insert(id, now.saturating_add(wait));
        });
        Ok(())
    }
}
