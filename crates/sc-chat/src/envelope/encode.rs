use sc_chat_wire::msg::ENVELOPE_VERSION;
use sc_chat_wire::proto::{self, Envelope, envelope::Body};

use super::snapshot::{embed_to_wire, meta_to_wire};
use super::{Content, Post, text};
use crate::contracts::{
    ChatError, ChatResult, EMBEDS_MAX, Embed, EmbedKind, Forwarded, RichText, Seq,
};

pub fn encode(content: &Content) -> ChatResult<Envelope> {
    let body = match content {
        Content::Post(post) => return post_envelope(post),
        Content::Edit { target, text } => Body::Edit(proto::Edit {
            target: wire_seq(*target)?,
            text: Some(edited_text(text)?),
        }),
        Content::Delete { target } => Body::Delete(proto::Delete {
            target: wire_seq(*target)?,
        }),
        Content::Reaction { target, emoji, on } => Body::Reaction(proto::Reaction {
            target: wire_seq(*target)?,
            emoji: checked_emoji(emoji)?,
            on: *on,
        }),
        Content::Meta(meta) => Body::Meta(meta_to_wire(meta)),
        Content::Pin { target, on } => Body::Pin(proto::Pin {
            target: wire_seq(*target)?,
            on: *on,
        }),
        Content::Suggest(track) => Body::Suggest(proto::Suggest {
            track: Some(Box::new(suggested(track)?)),
        }),
        Content::Unknown => return Err(ChatError::Invalid),
    };
    Ok(Envelope {
        v: ENVELOPE_VERSION,
        body: Some(body),
        ..Envelope::default()
    })
}

fn post_envelope(post: &Post) -> ChatResult<Envelope> {
    let empty = post.text.text.trim().is_empty() && post.embeds.is_empty();
    let blank_mention = post.mentions.iter().any(String::is_empty);
    if empty || blank_mention || post.embeds.len() > EMBEDS_MAX {
        return Err(ChatError::Invalid);
    }
    Ok(Envelope {
        v: ENVELOPE_VERSION,
        body: Some(Body::Text(text::to_wire(&post.text)?)),
        reply_to: post.reply_to.map(wire_seq).transpose()?.unwrap_or_default(),
        mentions: post.mentions.clone(),
        embeds: post.embeds.iter().map(embed_to_wire).collect(),
        forwarded_from: post.forwarded_from.as_ref().map(forwarded).transpose()?,
        ..Envelope::default()
    })
}

fn edited_text(text: &RichText) -> ChatResult<proto::Text> {
    if text.text.trim().is_empty() {
        return Err(ChatError::Invalid);
    }
    text::to_wire(text)
}

fn suggested(track: &Embed) -> ChatResult<proto::Embed> {
    match track.kind {
        EmbedKind::Track => Ok(embed_to_wire(track)),
        _ => Err(ChatError::Invalid),
    }
}

fn forwarded(from: &Forwarded) -> ChatResult<proto::Forwarded> {
    Ok(proto::Forwarded {
        user_id: from.user_id.clone(),
        name: from.name.clone(),
        sent_at_ms: u64::try_from(from.sent_at_ms).map_err(|_| ChatError::Invalid)?,
    })
}

fn checked_emoji(emoji: &str) -> ChatResult<String> {
    match text::is_reaction(emoji) {
        true => Ok(emoji.to_owned()),
        false => Err(ChatError::Invalid),
    }
}

fn wire_seq(seq: Seq) -> ChatResult<u64> {
    u64::try_from(seq)
        .ok()
        .filter(|seq| *seq > 0)
        .ok_or(ChatError::Invalid)
}
