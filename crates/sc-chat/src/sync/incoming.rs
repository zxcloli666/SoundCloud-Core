use std::collections::{BTreeSet, HashMap};

use super::conversations::RENDER_MAX;
use super::local::Local;
use super::{SyncEngine, rows, transient};
use crate::api::wire::MessageView;
use crate::contracts::{ChatResult, ConvId, HistoryQuery, Message, Seq};
use crate::crypto::Ck;
use crate::store::StoredMessage;

pub const CATCH_UP_LIMIT: u32 = 50;
pub const CATCH_UP_PAGES: usize = 4;

impl<L: Local> SyncEngine<L> {
    pub(super) async fn store_rows(
        &self,
        conversation_id: ConvId,
        views: &[MessageView],
    ) -> ChatResult<Vec<StoredMessage>> {
        let epochs: BTreeSet<u32> = views
            .iter()
            .filter(|view| rows::needs_key(view))
            .filter_map(rows::epoch_of)
            .collect();
        let mut keys: HashMap<u32, Ck> = HashMap::new();
        for epoch in epochs {
            if let Some(ck) = self.key(conversation_id, epoch).await? {
                keys.insert(epoch, ck);
            }
        }
        let stored: Vec<StoredMessage> = views
            .iter()
            .filter_map(|view| {
                let ck = rows::epoch_of(view).and_then(|epoch| keys.get(&epoch));
                rows::stored(conversation_id, view, ck, self.cipher.as_ref())
            })
            .collect();
        if !stored.is_empty() {
            self.store.put_messages(&stored)?;
        }
        Ok(stored)
    }

    pub(super) fn rendered(
        &self,
        conversation_id: ConvId,
        stored: &[StoredMessage],
    ) -> ChatResult<Vec<Message>> {
        let Some(lowest) = stored.iter().map(|message| message.seq).min() else {
            return Ok(Vec::new());
        };
        let fresh = HistoryQuery {
            before: None,
            after: Some(lowest.saturating_sub(1)),
            limit: RENDER_MAX,
        };
        let mut messages = self.store.messages(conversation_id, &fresh)?.messages;
        let targets: BTreeSet<Seq> = stored
            .iter()
            .filter_map(|message| message.target_seq)
            .filter(|target| *target < lowest)
            .collect();
        for target in targets {
            let one = HistoryQuery {
                before: None,
                after: Some(target.saturating_sub(1)),
                limit: 1,
            };
            let found = self.store.messages(conversation_id, &one)?.messages;
            messages.extend(
                found
                    .into_iter()
                    .filter(|message| message.seq == Some(target)),
            );
        }
        Ok(messages)
    }

    pub(super) async fn take_rows(
        &self,
        conversation_id: ConvId,
        views: &[MessageView],
    ) -> ChatResult<Vec<Message>> {
        let stored = self.store_rows(conversation_id, views).await?;
        self.rendered(conversation_id, &stored)
    }

    pub(super) async fn catch_up(
        &self,
        conversation_id: ConvId,
        local_last: Seq,
    ) -> ChatResult<Vec<Message>> {
        let mut messages = Vec::new();
        let mut after = (local_last > 0).then_some(local_last);
        for _ in 0..CATCH_UP_PAGES {
            let query = HistoryQuery {
                before: None,
                after,
                limit: CATCH_UP_LIMIT,
            };
            let page = match self.api.history(conversation_id, &query).await {
                Ok(page) => page,
                Err(error) if transient(&error) => return Err(error),
                Err(_) => break,
            };
            let newest = page.messages.iter().map(|message| message.seq).max();
            messages.extend(self.take_rows(conversation_id, &page.messages).await?);
            match (after, newest) {
                (Some(_), Some(newest)) if page.more => after = Some(newest),
                _ => break,
            }
        }
        Ok(messages)
    }
}
