use std::collections::BTreeSet;

use super::local::Local;
use super::{SyncEngine, rows, transient};
use crate::contracts::{ChatError, ChatResult, ConvId, CoreEvent, MessagesEvent, MessagesOrigin};
use crate::crypto::Ck;

pub const KEYS_PAGE_MAX: usize = 64;
pub const KEYS_PAGES_MAX: usize = 16;

impl<L: Local> SyncEngine<L> {
    pub(super) async fn key(&self, conversation_id: ConvId, epoch: u32) -> ChatResult<Option<Ck>> {
        if let Some(ck) = self.store.ck(conversation_id, epoch)? {
            return Ok(Some(ck));
        }
        if self.with(|state| state.locked_keys.contains(&(conversation_id, epoch))) {
            return Ok(None);
        }
        match self.fetch_keys(conversation_id, epoch).await {
            Ok(_) => {}
            Err(error) if transient(&error) => return Err(error),
            Err(_) => return Ok(None),
        }
        let ck = self.store.ck(conversation_id, epoch)?;
        if ck.is_none() {
            self.with(|state| state.locked_keys.insert((conversation_id, epoch)));
        }
        Ok(ck)
    }

    pub async fn fetch_keys(
        &self,
        conversation_id: ConvId,
        from_epoch: u32,
    ) -> ChatResult<Vec<u32>> {
        let mut from = from_epoch;
        let mut installed = BTreeSet::new();
        for _ in 0..KEYS_PAGES_MAX {
            let page = self.api.keys(conversation_id, Some(from)).await?;
            for key in &page {
                let Ok(epoch) = u32::try_from(key.epoch) else {
                    continue;
                };
                if let Ok(ck) = self.cipher.open_ck(conversation_id, epoch, &key.sealed) {
                    self.store.put_ck(conversation_id, epoch, &ck)?;
                    installed.insert(epoch);
                }
            }
            let newest = page
                .iter()
                .filter_map(|key| u32::try_from(key.epoch).ok())
                .max();
            match newest {
                Some(newest) if page.len() >= KEYS_PAGE_MAX && newest >= from => from = newest + 1,
                _ => break,
            }
        }
        if let Some(newest) = installed.last() {
            self.with(|state| {
                state.note_epoch(conversation_id, *newest);
                for epoch in &installed {
                    state.locked_keys.remove(&(conversation_id, *epoch));
                }
            });
        }
        for epoch in &installed {
            self.reopen(conversation_id, *epoch)?;
        }
        Ok(installed.into_iter().collect())
    }

    pub async fn refresh_keys(&self, conversation_id: ConvId) -> ChatResult<Vec<u32>> {
        self.fetch_keys(conversation_id, 0).await
    }

    pub(super) async fn send_key(
        &self,
        conversation_id: ConvId,
        stale: bool,
    ) -> ChatResult<(u32, Ck)> {
        let known = self.with(|state| state.epochs.get(&conversation_id).copied());
        if !stale
            && let Some(epoch) = known
            && let Some(ck) = self.store.ck(conversation_id, epoch)?
        {
            return Ok((epoch, ck));
        }
        let from = match (known, stale) {
            (Some(epoch), true) => epoch + 1,
            (Some(epoch), false) => epoch,
            (None, _) => 0,
        };
        self.fetch_keys(conversation_id, from).await?;
        let newest = self
            .with(|state| state.epochs.get(&conversation_id).copied())
            .ok_or(ChatError::Crypto)?;
        let ck = self
            .store
            .ck(conversation_id, newest)?
            .ok_or(ChatError::Crypto)?;
        Ok((newest, ck))
    }

    fn reopen(&self, conversation_id: ConvId, epoch: u32) -> ChatResult<()> {
        let sealed = self.store.sealed_messages(conversation_id, epoch)?;
        if sealed.is_empty() {
            return Ok(());
        }
        let Some(ck) = self.store.ck(conversation_id, epoch)? else {
            return Ok(());
        };
        let opened: Vec<_> = sealed
            .into_iter()
            .map(|message| rows::opened(message, &ck, self.cipher.as_ref()))
            .collect();
        self.store.put_messages(&opened)?;
        let messages = self.rendered(conversation_id, &opened)?;
        if !messages.is_empty() {
            self.events.emit(CoreEvent::Messages(MessagesEvent {
                conversation_id,
                origin: MessagesOrigin::History,
                messages,
            }));
        }
        Ok(())
    }
}
