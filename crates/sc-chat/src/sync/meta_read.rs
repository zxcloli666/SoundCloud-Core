use super::SyncEngine;
use super::local::Local;
use crate::api::wire::ConversationView;
use crate::contracts::{ChatResult, ConversationMeta};
use crate::envelope::{self, Content};

pub(super) enum MetaRead {
    Absent,
    Locked(Option<u32>),
    Open(Box<ConversationMeta>),
}

impl MetaRead {
    pub(super) fn opened(self) -> Option<ConversationMeta> {
        match self {
            MetaRead::Open(meta) => Some(*meta),
            MetaRead::Absent | MetaRead::Locked(_) => None,
        }
    }
}

impl<L: Local> SyncEngine<L> {
    pub(super) async fn read_meta(&self, view: &ConversationView) -> ChatResult<MetaRead> {
        let (Some(meta_ct), Some(epoch)) = (&view.meta_ct, view.meta_epoch) else {
            return Ok(MetaRead::Absent);
        };
        let Ok(epoch) = u32::try_from(epoch) else {
            return Ok(MetaRead::Locked(None));
        };
        let Some(ck) = self.key(view.id, epoch).await? else {
            return Ok(MetaRead::Locked(Some(epoch)));
        };
        let Ok(envelope) = self.cipher.open_meta(&ck, view.id, epoch, meta_ct) else {
            return Ok(MetaRead::Locked(None));
        };
        Ok(match envelope::decode(&envelope) {
            Ok(Content::Meta(meta)) => MetaRead::Open(meta),
            _ => MetaRead::Locked(None),
        })
    }
}
