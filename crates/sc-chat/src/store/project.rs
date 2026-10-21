use super::content;
use super::items::Item;
use super::{StoredContent, StoredMessage};
use crate::contracts::{
    DeliveryState, Message, MessageBody, MessageClass, PeerMark, ReactionGroup, RichText, SpanKind,
};
use crate::envelope::Content;

#[derive(Default, Clone, Copy)]
pub struct Context<'a> {
    pub me: Option<&'a str>,
    pub marks: &'a [PeerMark],
}

pub fn visible(class: MessageClass) -> bool {
    matches!(class, MessageClass::Post | MessageClass::System)
}

pub fn message(item: &Item, modifiers: &[&Item], context: Context<'_>) -> Message {
    let row = &item.message;
    let mut message = Message {
        conversation_id: row.conversation_id,
        seq: Some(row.seq),
        client_id: row.client_id,
        sender: row.sender.clone(),
        sender_device: row.sender_device,
        mine: context.me == Some(row.sender.as_str()),
        ts: row.ts,
        edited_at: None,
        expires_at: row.expires_at,
        state: DeliveryState::Sent,
        body: MessageBody::Unknown,
        reply_to: None,
        mentions: Vec::new(),
        mentions_me: false,
        embeds: Vec::new(),
        forwarded_from: None,
        reactions: Vec::new(),
        pinned: false,
        local_only: row.local_only,
    };
    fill_body(&mut message, &row.content);
    for modifier in modifiers {
        apply(&mut message, modifier);
    }
    message.reactions = reactions(modifiers, context.me);
    message.mentions_me = mentions(&message, context.me);
    message.state = state(&message, row.seq, context);
    message
}

pub fn fill_body(message: &mut Message, stored: &StoredContent) {
    message.body = match stored {
        StoredContent::Open(bytes) => match content::decode(bytes) {
            Content::Post(post) => {
                message.reply_to = post.reply_to;
                message.mentions = post.mentions;
                message.embeds = post.embeds;
                message.forwarded_from = post.forwarded_from;
                MessageBody::Text(post.text)
            }
            Content::Suggest(embed) => {
                message.embeds = vec![embed];
                MessageBody::Text(RichText::default())
            }
            _ => MessageBody::Unknown,
        },
        StoredContent::Sealed(_) => MessageBody::Locked,
        StoredContent::System(event) => MessageBody::System(event.clone()),
        StoredContent::Deleted => MessageBody::Deleted,
    };
}

fn apply(message: &mut Message, modifier: &Item) {
    let row = &modifier.message;
    let StoredContent::Open(bytes) = &row.content else {
        return;
    };
    let aims = |declared: i64| aims_at(row, declared, message.seq);
    let same_sender = row.sender == message.sender;
    match content::decode(bytes) {
        Content::Edit { target, text } if aims(target) && same_sender => {
            if matches!(message.body, MessageBody::Text(_)) {
                message.body = MessageBody::Text(text);
                message.edited_at = Some(row.ts);
            }
        }
        Content::Delete { target } if aims(target) && same_sender => {
            message.body = MessageBody::Deleted;
            message.embeds.clear();
            message.mentions.clear();
            message.forwarded_from = None;
        }
        Content::Pin { target, on } if aims(target) => message.pinned = on,
        _ => {}
    }
}

pub fn aims_at(row: &StoredMessage, declared: i64, seq: Option<i64>) -> bool {
    row.target_seq == seq && (row.local_only || Some(declared) == seq)
}

fn reactions(modifiers: &[&Item], me: Option<&str>) -> Vec<ReactionGroup> {
    let mut latest: Vec<(String, String, bool)> = Vec::new();
    for modifier in modifiers {
        let row = &modifier.message;
        let StoredContent::Open(bytes) = &row.content else {
            continue;
        };
        let Content::Reaction { target, emoji, on } = content::decode(bytes) else {
            continue;
        };
        if !aims_at(row, target, row.target_seq) {
            continue;
        }
        match latest
            .iter_mut()
            .find(|(sender, seen, _)| *sender == row.sender && *seen == emoji)
        {
            Some(entry) => entry.2 = on,
            None => latest.push((row.sender.clone(), emoji, on)),
        }
    }
    let mut groups: Vec<ReactionGroup> = Vec::new();
    for (sender, emoji, _) in latest.into_iter().filter(|entry| entry.2) {
        let mine = me == Some(sender.as_str());
        match groups.iter_mut().find(|group| group.emoji == emoji) {
            Some(group) => {
                group.count += 1;
                group.mine |= mine;
            }
            None => groups.push(ReactionGroup {
                emoji,
                count: 1,
                mine,
            }),
        }
    }
    groups
}

fn mentions(message: &Message, me: Option<&str>) -> bool {
    let Some(me) = me else {
        return false;
    };
    let in_spans = match &message.body {
        MessageBody::Text(text) => text
            .spans
            .iter()
            .any(|span| span.kind == SpanKind::Mention && span.user_id.as_deref() == Some(me)),
        _ => false,
    };
    in_spans || message.mentions.iter().any(|user| user == me)
}

fn state(message: &Message, seq: i64, context: Context<'_>) -> DeliveryState {
    if !message.mine || message.local_only {
        return DeliveryState::Sent;
    }
    let peers = context
        .marks
        .iter()
        .filter(|mark| Some(mark.user_id.as_str()) != context.me);
    let mut state = DeliveryState::Sent;
    for mark in peers {
        if mark.read_seq >= seq {
            return DeliveryState::Read;
        }
        if mark.delivered_seq >= seq {
            state = DeliveryState::Delivered;
        }
    }
    state
}

pub fn searchable(message: &Message) -> Option<String> {
    let MessageBody::Text(text) = &message.body else {
        return None;
    };
    let mut parts = vec![text.text.clone()];
    for embed in &message.embeds {
        parts.push(embed.title.clone());
        parts.extend(embed.artist.clone());
    }
    let joined = parts.join("\n");
    (!joined.trim().is_empty()).then_some(joined)
}
