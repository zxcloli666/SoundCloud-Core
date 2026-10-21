use sc_chat_wire::proto::{self, Envelope, envelope::Body};

use super::snapshot::{embed_from_wire, meta_from_wire};
use super::{Content, Post, text};
use crate::contracts::{
    ChatError, ChatResult, EMBEDS_MAX, Embed, EmbedKind, Forwarded, RichText, Seq,
};

pub fn decode(envelope: &Envelope) -> ChatResult<Content> {
    let Some(body) = &envelope.body else {
        return Ok(Content::Unknown);
    };
    Ok(match body {
        Body::Text(text) => Content::Post(post(envelope, text)?),
        Body::Edit(edit) => Content::Edit {
            target: local_seq(edit.target)?,
            text: edited_text(edit.text.clone())?,
        },
        Body::Delete(delete) => Content::Delete {
            target: local_seq(delete.target)?,
        },
        Body::Reaction(reaction) => Content::Reaction {
            target: local_seq(reaction.target)?,
            emoji: emoji(&reaction.emoji)?,
            on: reaction.on,
        },
        Body::Meta(meta) => Content::Meta(meta_from_wire(meta.clone())),
        Body::Pin(pin) => Content::Pin {
            target: local_seq(pin.target)?,
            on: pin.on,
        },
        Body::Suggest(suggest) => Content::Suggest(suggested(suggest)?),
    })
}

fn post(envelope: &Envelope, body: &proto::Text) -> ChatResult<Post> {
    Ok(Post {
        text: text::from_wire(body.clone())?,
        reply_to: match envelope.reply_to {
            0 => None,
            seq => Some(local_seq(seq)?),
        },
        mentions: envelope
            .mentions
            .iter()
            .filter(|id| !id.is_empty())
            .cloned()
            .collect(),
        embeds: envelope
            .embeds
            .iter()
            .cloned()
            .filter_map(embed_from_wire)
            .take(EMBEDS_MAX)
            .collect(),
        forwarded_from: envelope.forwarded_from.clone().map(forwarded).transpose()?,
    })
}

fn edited_text(body: Option<proto::Text>) -> ChatResult<RichText> {
    let text = text::from_wire(body.ok_or(ChatError::Protocol)?)?;
    match text.text.trim().is_empty() {
        true => Err(ChatError::Protocol),
        false => Ok(text),
    }
}

fn suggested(suggest: &proto::Suggest) -> ChatResult<Embed> {
    let track = suggest.track.as_deref().cloned().and_then(embed_from_wire);
    track
        .filter(|track| track.kind == EmbedKind::Track)
        .ok_or(ChatError::Protocol)
}

fn forwarded(from: proto::Forwarded) -> ChatResult<Forwarded> {
    Ok(Forwarded {
        user_id: from.user_id,
        name: from.name,
        sent_at_ms: i64::try_from(from.sent_at_ms).map_err(|_| ChatError::Protocol)?,
    })
}

fn emoji(emoji: &str) -> ChatResult<String> {
    match text::is_reaction(emoji) {
        true => Ok(emoji.to_owned()),
        false => Err(ChatError::Protocol),
    }
}

fn local_seq(seq: u64) -> ChatResult<Seq> {
    i64::try_from(seq)
        .ok()
        .filter(|seq| *seq > 0)
        .ok_or(ChatError::Protocol)
}
