use super::local::Local;
use super::{SyncEngine, summary, transient};
use crate::contracts::{
    ChatError, ChatResult, ConvId, ConversationKind, ConversationSummary, CoreEvent, HistoryQuery,
    MemberRole, MemberState, MessagesEvent, MessagesOrigin, Millis, NotifyLevel, Seq,
};

impl<L: Local> SyncEngine<L> {
    pub async fn adopt_room(
        &self,
        id: ConvId,
        code: &str,
        from_seq: Seq,
    ) -> ChatResult<ConversationSummary> {
        let mut summary = match self.store.conversation(id)? {
            Some(existing) => ConversationSummary {
                room_code: Some(code.to_owned()),
                ..existing
            },
            None => seated(id, code, from_seq, self.now()),
        };
        let messages = self.catch_up(id, summary.last_seq).await?;
        summary::absorb(&mut summary, &messages);
        self.save(std::slice::from_ref(&summary))?;
        self.emit_summaries(vec![summary.clone()]);
        if !messages.is_empty() {
            self.events.emit(CoreEvent::Messages(MessagesEvent {
                conversation_id: id,
                origin: MessagesOrigin::Sync,
                messages,
            }));
        }
        Ok(summary)
    }

    pub(super) async fn catch_up_rooms(
        &self,
        local: &[(ConvId, Seq)],
    ) -> ChatResult<Vec<ConversationSummary>> {
        let mut rooms = Vec::new();
        let mut ended = Vec::new();
        for (id, _) in local {
            let Some(mut summary) = self.store.conversation(*id)? else {
                continue;
            };
            if summary.kind != ConversationKind::Room {
                continue;
            }
            if self.seat_lost(*id, summary.last_seq).await? {
                ended.push(*id);
                continue;
            }
            let messages = self.catch_up(*id, summary.last_seq).await?;
            summary::absorb(&mut summary, &messages);
            self.save(std::slice::from_ref(&summary))?;
            if !messages.is_empty() {
                self.events.emit(CoreEvent::Messages(MessagesEvent {
                    conversation_id: *id,
                    origin: MessagesOrigin::Sync,
                    messages,
                }));
            }
            rooms.push(summary);
        }
        self.purge(&ended, &[])?;
        Ok(rooms)
    }

    async fn seat_lost(&self, id: ConvId, last_seq: Seq) -> ChatResult<bool> {
        let probe = HistoryQuery {
            before: None,
            after: Some(last_seq.max(0)),
            limit: 1,
        };
        match self.api.history(id, &probe).await {
            Ok(_) => Ok(false),
            Err(ChatError::NotMember | ChatError::NotFound) => Ok(true),
            Err(error) if transient(&error) => Err(error),
            Err(_) => Ok(false),
        }
    }

    pub(super) fn is_room(&self, id: ConvId) -> ChatResult<bool> {
        Ok(self
            .store
            .conversation(id)?
            .is_some_and(|summary| summary.kind == ConversationKind::Room))
    }
}

fn seated(id: ConvId, code: &str, from_seq: Seq, now: Millis) -> ConversationSummary {
    ConversationSummary {
        id,
        kind: ConversationKind::Room,
        title: None,
        avatar_url: None,
        peer: None,
        role: MemberRole::Member,
        state: MemberState::Active,
        accepted: true,
        member_count: 0,
        last_seq: from_seq,
        last_at: now,
        last_read_seq: from_seq,
        unread: 0,
        mentions: 0,
        preview: None,
        muted_until: None,
        notify: NotifyLevel::All,
        pinned_at: None,
        archived: false,
        ttl_s: None,
        room_code: Some(code.to_owned()),
        history_locked: false,
        has_draft: false,
    }
}
