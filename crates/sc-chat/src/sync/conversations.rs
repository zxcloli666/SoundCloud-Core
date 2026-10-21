use sc_chat_wire::proto::envelope::Body;

use super::local::Local;
use super::summary::{self, Facts};
use super::{SyncEngine, fatal, system, transient};
use crate::api::wire::{CardView, ConversationView, MessageView, SyncConversation};
use crate::contracts::{
    ChatError, ChatResult, ConvId, ConversationKind, ConversationSummary, CoreEvent, HistoryQuery,
    Seq, SyncEvent, SystemKind, UserId, UserRef,
};
use crate::embeds::clean;

pub const RENDER_MAX: u32 = 200;

fn structural(message: &MessageView) -> bool {
    match (message.class.as_str(), &message.body) {
        ("meta", _) => true,
        ("system", Some(body)) => !matches!(
            system::parse(body).kind,
            SystemKind::Deleted | SystemKind::Track | SystemKind::Suggest
        ),
        ("system", None) => true,
        _ => false,
    }
}

impl<L: Local> SyncEngine<L> {
    pub async fn refresh_conversation(&self, id: ConvId) -> ChatResult<ConversationSummary> {
        let view = self.api.conversation(id).await?;
        self.adopt(&view).await
    }

    pub async fn adopt(&self, view: &ConversationView) -> ChatResult<ConversationSummary> {
        self.note_view(view);
        let existing = self.store.conversation(view.id)?;
        let summary = self.summary_of(view, existing.as_ref()).await?;
        self.save(std::slice::from_ref(&summary))?;
        self.emit_summaries(vec![summary.clone()]);
        Ok(summary)
    }

    pub(super) async fn summary_for(
        &self,
        entry: &SyncConversation,
        existing: Option<ConversationSummary>,
        behind: bool,
    ) -> ChatResult<Option<ConversationSummary>> {
        let structural = entry.messages.iter().any(structural);
        let bump = entry.messages.is_empty()
            && entry.typing.is_empty()
            && !self.marks_moved(entry, existing.as_ref());
        let room = existing
            .as_ref()
            .is_some_and(|summary| summary.kind == ConversationKind::Room);
        let refresh = existing.is_none() || (!room && (structural || bump || behind));
        if !refresh {
            return Ok(existing);
        }
        match self.api.conversation(entry.id).await {
            Ok(view) => {
                self.note_view(&view);
                Ok(Some(self.summary_of(&view, existing.as_ref()).await?))
            }
            Err(error) if transient(&error) => Err(error),
            Err(_) => Ok(existing),
        }
    }

    fn marks_moved(
        &self,
        entry: &SyncConversation,
        existing: Option<&ConversationSummary>,
    ) -> bool {
        let me = self.with(|state| state.user.clone());
        let read = existing.map(|summary| summary.last_read_seq);
        entry.marks.iter().any(|mark| {
            me.as_ref() != Some(&mark.user_id) || read.is_none_or(|read| mark.read_seq > read)
        })
    }

    pub(super) async fn summary_of(
        &self,
        view: &ConversationView,
        existing: Option<&ConversationSummary>,
    ) -> ChatResult<ConversationSummary> {
        let title = self.meta_title(view).await?;
        let peer = match &view.peer {
            Some(peer) => Some(self.user_ref(peer).await?),
            None => None,
        };
        let history_locked = self.with(|state| {
            state.status.history.locked
                && state
                    .device_created_at
                    .is_some_and(|created| view.created_at < created)
        });
        let facts = Facts {
            title,
            peer,
            history_locked,
        };
        Ok(summary::from_view(view, existing, facts))
    }

    pub(super) fn note_view(&self, view: &ConversationView) {
        if let Ok(epoch) = u32::try_from(view.key_epoch) {
            self.with(|state| state.note_epoch(view.id, epoch));
        }
    }

    async fn meta_title(&self, view: &ConversationView) -> ChatResult<Option<String>> {
        let (Some(meta_ct), Some(epoch)) = (&view.meta_ct, view.meta_epoch) else {
            return Ok(None);
        };
        let Ok(epoch) = u32::try_from(epoch) else {
            return Ok(None);
        };
        let Some(ck) = self.key(view.id, epoch).await? else {
            return Ok(None);
        };
        let Ok(envelope) = self.cipher.open_meta(&ck, view.id, epoch, meta_ct) else {
            return Ok(None);
        };
        Ok(match envelope.body {
            Some(Body::Meta(meta)) => clean::optional_text(Some(&meta.title), clean::TEXT_MAX),
            _ => None,
        })
    }

    async fn user_ref(&self, user_id: &UserId) -> ChatResult<UserRef> {
        if let Some(found) = self.store.users(std::slice::from_ref(user_id))?.pop() {
            return Ok(found);
        }
        match self.api.user_card(user_id).await {
            Ok(card) => {
                let user = user_of(card);
                self.store.put_users(std::slice::from_ref(&user))?;
                Ok(user)
            }
            Err(error) if fatal(&error) || error == ChatError::Offline => Err(error),
            Err(_) => Ok(UserRef {
                id: user_id.clone(),
                name: String::new(),
                avatar_url: None,
                permalink: None,
            }),
        }
    }

    pub(super) fn read_locally(&self, conversation_id: ConvId, seq: Seq) {
        let Ok(Some(mut summary)) = self.store.conversation(conversation_id) else {
            return;
        };
        if seq <= summary.last_read_seq {
            return;
        }
        summary.last_read_seq = seq;
        if self.recount(&mut summary).is_ok() && self.save(std::slice::from_ref(&summary)).is_ok() {
            self.emit_summaries(vec![summary]);
        }
    }

    pub(super) fn recount(&self, summary: &mut ConversationSummary) -> ChatResult<()> {
        if summary.last_read_seq >= summary.last_seq {
            summary.unread = 0;
            summary.mentions = 0;
            return Ok(());
        }
        let query = HistoryQuery {
            before: None,
            after: Some(summary.last_read_seq),
            limit: RENDER_MAX,
        };
        let unread = self.store.messages(summary.id, &query)?.messages;
        summary::recount(summary, &unread);
        Ok(())
    }

    pub(super) fn refresh_preview(&self, conversation_id: ConvId) -> ChatResult<()> {
        let Some(mut summary) = self.store.conversation(conversation_id)? else {
            return Ok(());
        };
        let newest = HistoryQuery {
            before: None,
            after: None,
            limit: 1,
        };
        let latest = self.store.messages(conversation_id, &newest)?.messages;
        summary.preview = latest
            .last()
            .and_then(|message| Some(summary::preview(message, message.seq?)));
        self.recount(&mut summary)?;
        self.save(std::slice::from_ref(&summary))?;
        self.emit_summaries(vec![summary]);
        Ok(())
    }

    pub(super) fn emit_summaries(&self, conversations: Vec<ConversationSummary>) {
        let requests = self.with(|state| state.status.requests);
        self.events.emit(CoreEvent::Sync(SyncEvent {
            reset: false,
            conversations,
            marks: Vec::new(),
            requests,
        }));
    }
}

fn user_of(card: CardView) -> UserRef {
    UserRef {
        name: card.username.unwrap_or_default(),
        id: card.user_id,
        avatar_url: card.avatar_url,
        permalink: card.permalink_url,
    }
}
