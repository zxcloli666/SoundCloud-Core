use uuid::Uuid;

use super::conversations::{MetaRead, avatar_id};
use super::local::Local;
use super::{SyncEngine, fatal, transient};
use crate::api::wire::MessageView;
use crate::contracts::{ChatError, ChatResult, ConvId, MessageClass};

type Backdrop = Option<Uuid>;

impl<L: Local> SyncEngine<L> {
    pub(super) fn note_room_meta(&self, id: ConvId, views: &[MessageView]) -> ChatResult<()> {
        let carries_meta = views
            .iter()
            .any(|view| view.class == MessageClass::Meta.as_str());
        if carries_meta && self.is_room(id)? {
            self.room_meta_due(id);
        }
        Ok(())
    }

    pub(super) fn room_meta_due(&self, id: ConvId) {
        self.with(|state| state.room_meta_due.insert(id));
    }

    pub(super) async fn settle_room_meta(&self, id: ConvId) -> ChatResult<Option<Backdrop>> {
        if !self.with(|state| state.room_meta_due.remove(&id)) {
            return Ok(None);
        }
        let view = match self.api.conversation(id).await {
            Ok(view) => view,
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.note_view(&view);
        let meta = match self.read_meta(&view).await {
            Ok(MetaRead::Open(meta)) => Some(*meta),
            Ok(MetaRead::Absent) => None,
            Ok(MetaRead::Locked) => return Ok(None),
            Err(error) => return self.room_meta_missed(id, error),
        };
        self.store.put_room_meta(id, meta.as_ref())?;
        Ok(Some(meta.as_ref().and_then(avatar_id)))
    }

    fn room_meta_missed(&self, id: ConvId, error: ChatError) -> ChatResult<Option<Backdrop>> {
        if fatal(&error) {
            return Err(error);
        }
        if transient(&error) {
            self.room_meta_due(id);
        }
        Ok(None)
    }
}
