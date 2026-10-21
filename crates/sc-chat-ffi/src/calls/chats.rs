use sc_chat::api::wire::NewConversation;
use sc_chat::envelope::Content;
use sc_chat::{
    ChatError, ChatResult, ConvId, ConversationKind, ConversationMeta, ConversationPage,
    ConversationSummary, CreateConversation, HistoryPage, HistoryQuery, ListQuery, ThreadSnapshot,
};

use crate::runtime::{Chat, Core};

const LIST_MAX: u32 = 200;
const HISTORY_MAX: u32 = 200;
const OPEN_PAGE: u32 = 50;

impl Chat {
    pub async fn conversations(&self, query: ListQuery) -> ChatResult<ConversationPage> {
        let query = ListQuery {
            limit: query.limit.clamp(1, LIST_MAX),
            ..query
        };
        self.blocking(move |core| core.store.conversations(&query))
            .await
    }

    pub async fn open(&self, conversation_id: ConvId) -> ChatResult<ThreadSnapshot> {
        self.blocking(move |core| snapshot(core, conversation_id))
            .await
    }

    pub async fn history(
        &self,
        conversation_id: ConvId,
        query: HistoryQuery,
    ) -> ChatResult<HistoryPage> {
        let query = HistoryQuery {
            limit: query.limit.clamp(1, HISTORY_MAX),
            ..query
        };
        let local = self
            .blocking(move |core| core.store.messages(conversation_id, &query))
            .await?;
        if query.after.is_some() || local.messages.len() >= query.limit as usize {
            return Ok(local);
        }
        let core = self.ready()?;
        match core.sync.fetch_history(conversation_id, &query).await {
            Ok(page) => Ok(page),
            Err(_) => Ok(local),
        }
    }

    pub async fn create(&self, input: CreateConversation) -> ChatResult<ConversationSummary> {
        let core = self.ready()?;
        let body = NewConversation {
            kind: input.kind,
            members: input.members,
            meta_ct: None,
            meta_epoch: None,
        };
        let view = self.api.create_conversation(&body).await?;
        let summary = core.sync.adopt(&view).await?;
        let title = input.title.filter(|title| !title.trim().is_empty());
        if let (ConversationKind::Group, Some(title)) = (input.kind, title) {
            let meta = ConversationMeta {
                title,
                description: String::new(),
                avatar_media_id: None,
            };
            self.queue(view.id, None, Content::Meta(meta)).await?;
        }
        Ok(summary)
    }
}

fn snapshot(core: &Core, conversation_id: ConvId) -> ChatResult<ThreadSnapshot> {
    let conversation = core
        .store
        .conversation(conversation_id)?
        .ok_or(ChatError::NotFound)?;
    let latest = HistoryQuery {
        before: None,
        after: None,
        limit: OPEN_PAGE,
    };
    let page = core.store.messages(conversation_id, &latest)?;
    let mut messages = page.messages;
    messages.extend(core.outbox.pending(conversation_id)?);
    let pins = messages
        .iter()
        .filter(|message| message.pinned)
        .filter_map(|message| message.seq)
        .collect();
    Ok(ThreadSnapshot {
        conversation,
        more_before: page.more,
        marks: core.store.marks(conversation_id)?,
        pins,
        typing: Vec::new(),
        draft: core.store.draft(conversation_id)?,
        messages,
    })
}
