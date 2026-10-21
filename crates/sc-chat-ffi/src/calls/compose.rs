use sc_chat::envelope::{self, Content, Post};
use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::{
    ChatError, ChatResult, ClientId, ConvId, ConversationKind, EMBEDS_MAX, Forwarded, HistoryQuery,
    Message, MessageBody, MessageRef, RichText, SendInput, Seq, SpanKind, TEXT_MAX, UserId, embeds,
};

use crate::runtime::{Chat, Core};

impl Chat {
    pub async fn send(&self, input: SendInput) -> ChatResult<Message> {
        let post = match input.forward {
            Some(source) => self.blocking(move |core| forward_of(core, source)).await?,
            None => Post {
                mentions: mentions(&input.text),
                embeds: embeds::validate_all(&input.embeds)
                    .into_iter()
                    .take(EMBEDS_MAX)
                    .collect(),
                text: input.text,
                reply_to: input.reply_to,
                forwarded_from: None,
            },
        };
        if post.text.text.trim().is_empty() && post.embeds.is_empty() {
            return Err(ChatError::Invalid);
        }
        if post.text.text.encode_utf16().count() > TEXT_MAX {
            return Err(ChatError::Invalid);
        }
        self.queue(input.conversation_id, input.client_id, Content::Post(post))
            .await
    }

    pub async fn mark_read(&self, conversation_id: ConvId, seq: Seq) -> ChatResult<()> {
        self.blocking(move |core| {
            core.sync.queue_read(conversation_id, seq);
            Ok(())
        })
        .await
    }

    pub async fn queue(
        &self,
        conversation_id: ConvId,
        client_id: Option<ClientId>,
        content: Content,
    ) -> ChatResult<Message> {
        let envelope = envelope::encode(&content)?.encode_to_vec();
        let now = self.now_ms();
        let item = OutboxItem {
            client_id: client_id.unwrap_or_else(uuid::Uuid::new_v4),
            conversation_id,
            class: content.class(),
            target_seq: content.target(),
            envelope,
            created_at: now,
            attempts: 0,
            next_at: now,
        };
        let message = self.blocking(move |core| core.outbox.push(item)).await?;
        self.kick();
        Ok(message)
    }
}

fn forward_of(core: &Core, source: MessageRef) -> ChatResult<Post> {
    let conversation = core
        .store
        .conversation(source.conversation_id)?
        .ok_or(ChatError::NotFound)?;
    if conversation.kind == ConversationKind::Room {
        return Err(ChatError::Invalid);
    }
    let exact = HistoryQuery {
        before: None,
        after: Some(source.seq - 1),
        limit: 1,
    };
    let message = core
        .store
        .messages(source.conversation_id, &exact)?
        .messages
        .into_iter()
        .find(|message| message.seq == Some(source.seq))
        .ok_or(ChatError::NotFound)?;
    let MessageBody::Text(text) = message.body else {
        return Err(ChatError::Invalid);
    };
    let author = core.store.users(std::slice::from_ref(&message.sender))?;
    let forwarded_from = message.forwarded_from.unwrap_or_else(|| Forwarded {
        name: author
            .first()
            .map_or_else(|| message.sender.clone(), |user| user.name.clone()),
        user_id: message.sender,
        sent_at_ms: message.ts,
    });
    Ok(Post {
        mentions: Vec::new(),
        embeds: message.embeds,
        text,
        reply_to: None,
        forwarded_from: Some(forwarded_from),
    })
}

fn mentions(text: &RichText) -> Vec<UserId> {
    let mut users: Vec<UserId> = Vec::new();
    for span in &text.spans {
        if span.kind == SpanKind::Mention
            && let Some(user) = &span.user_id
            && !users.contains(user)
        {
            users.push(user.clone());
        }
    }
    users
}
