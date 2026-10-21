mod decode;
mod encode;
mod snapshot;
mod text;

use crate::contracts::{
    ChatError, ChatResult, ConversationMeta, Embed, Forwarded, MessageClass, RichText, Seq, UserId,
};
use crate::crypto::{Ck, MessageAt, open_message, seal_message};

pub use decode::decode;
pub use encode::encode;

pub const REACTION_MAX: usize = 64;
pub const LINK_MAX: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub text: RichText,
    pub reply_to: Option<Seq>,
    pub mentions: Vec<UserId>,
    pub embeds: Vec<Embed>,
    pub forwarded_from: Option<Forwarded>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Post(Post),
    Edit {
        target: Seq,
        text: RichText,
    },
    Delete {
        target: Seq,
    },
    Reaction {
        target: Seq,
        emoji: String,
        on: bool,
    },
    Meta(ConversationMeta),
    Pin {
        target: Seq,
        on: bool,
    },
    Suggest(Embed),
    Unknown,
}

impl Content {
    pub fn class(&self) -> MessageClass {
        match self {
            Content::Post(_) | Content::Suggest(_) | Content::Unknown => MessageClass::Post,
            Content::Edit { .. } | Content::Delete { .. } => MessageClass::Edit,
            Content::Reaction { .. } => MessageClass::React,
            Content::Meta(_) => MessageClass::Meta,
            Content::Pin { .. } => MessageClass::Pin,
        }
    }

    pub fn target(&self) -> Option<Seq> {
        match self {
            Content::Edit { target, .. }
            | Content::Delete { target }
            | Content::Reaction { target, .. }
            | Content::Pin { target, .. } => Some(*target),
            Content::Post(_) | Content::Meta(_) | Content::Suggest(_) | Content::Unknown => None,
        }
    }
}

pub fn seal(ck: &Ck, at: &MessageAt, content: &Content) -> ChatResult<Vec<u8>> {
    if content.class() != at.class {
        return Err(ChatError::Invalid);
    }
    seal_message(ck, at, &encode(content)?)
}

pub fn open(ck: &Ck, at: &MessageAt, target: Option<Seq>, body: &[u8]) -> ChatResult<Content> {
    let content = decode(&open_message(ck, at, body)?)?;
    match content {
        Content::Unknown => Ok(content),
        _ if content.target() == target => Ok(content),
        _ => Err(ChatError::Protocol),
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
