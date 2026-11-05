use sc_chat::envelope::{self, Content, Post};
use sc_chat::media::{ALBUM_MAX, ROOM_ALBUM_MAX};
use sc_chat::outbox::OutboxItem;
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::sc_chat_wire::proto::Attachment;
use sc_chat::{
    ChatError, ChatResult, ClientId, ConvId, ConversationKind, EMBEDS_MAX, Forwarded, HistoryQuery,
    MediaKind, Message, MessageBody, MessageRef, RichText, SendInput, Seq, SpanKind, TEXT_MAX,
    UploadState, UserId, embeds,
};
use uuid::Uuid;

use crate::runtime::{Chat, Core};

impl Chat {
    pub async fn send(&self, input: SendInput) -> ChatResult<Message> {
        let post = match input.forward {
            Some(source) => self.blocking(move |core| forward_of(core, source)).await?,
            None => composed(&input, Vec::new()),
        };
        self.queue_post(&input, post).await
    }

    pub async fn send_album(&self, input: SendInput, album: Uuid) -> ChatResult<Message> {
        if input.forward.is_some() {
            return Err(ChatError::Invalid);
        }
        let conversation_id = input.conversation_id;
        let attachments = self
            .blocking(move |core| album_of(core, conversation_id, album))
            .await?;
        let post = composed(&input, attachments);
        self.queue_post(&input, post).await
    }

    async fn queue_post(&self, input: &SendInput, post: Post) -> ChatResult<Message> {
        if post.text.text.trim().is_empty() && post.embeds.is_empty() && post.attachments.is_empty()
        {
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

fn composed(input: &SendInput, attachments: Vec<Attachment>) -> Post {
    Post {
        mentions: mentions(&input.text),
        embeds: embeds::validate_all(&input.embeds)
            .into_iter()
            .take(EMBEDS_MAX)
            .collect(),
        attachments,
        text: input.text.clone(),
        reply_to: input.reply_to,
        forwarded_from: None,
    }
}

fn album_of(core: &Core, conversation_id: ConvId, album: Uuid) -> ChatResult<Vec<Attachment>> {
    let uploads = core.store.album_uploads(album)?;
    if uploads.is_empty() {
        return Err(ChatError::NotFound);
    }
    let foreign = uploads.iter().any(|upload| {
        upload.kind != MediaKind::Image
            || upload.conversation_id != conversation_id
            || upload.state == UploadState::Failed
    });
    let room = core
        .store
        .conversation(conversation_id)?
        .is_some_and(|summary| summary.kind == ConversationKind::Room);
    let cap = if room { ROOM_ALBUM_MAX } else { ALBUM_MAX };
    if foreign || uploads.len() > cap {
        return Err(ChatError::Invalid);
    }
    if uploads
        .iter()
        .any(|upload| upload.state != UploadState::Committed)
    {
        return Err(ChatError::NotReady);
    }
    Ok(uploads
        .into_iter()
        .map(|upload| upload.attachment)
        .collect())
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
    let attachments = message
        .attachments
        .iter()
        .map(|attachment| {
            core.store
                .attachment(source.conversation_id, attachment.media_id)?
                .ok_or(ChatError::NotFound)
        })
        .collect::<ChatResult<Vec<_>>>()?;
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
        attachments,
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
