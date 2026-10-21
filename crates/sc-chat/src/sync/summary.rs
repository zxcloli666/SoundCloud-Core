use crate::api::wire::ConversationView;
use crate::contracts::{
    ConversationSummary, MemberRole, MemberState, Message, MessageBody, MessagePreview,
    NotifyLevel, PreviewKind, SystemKind, UserRef,
};

pub const PREVIEW_CHARS: usize = 120;

pub struct Facts {
    pub title: Option<String>,
    pub peer: Option<UserRef>,
    pub history_locked: bool,
}

pub fn from_view(
    view: &ConversationView,
    existing: Option<&ConversationSummary>,
    facts: Facts,
) -> ConversationSummary {
    let me = &view.me;
    let last_read_seq = existing
        .map(|summary| summary.last_read_seq)
        .unwrap_or_default()
        .max(me.last_read_seq);
    ConversationSummary {
        id: view.id,
        kind: view.kind,
        title: facts
            .title
            .or_else(|| existing.and_then(|summary| summary.title.clone())),
        avatar_url: facts
            .peer
            .as_ref()
            .and_then(|peer| peer.avatar_url.clone())
            .or_else(|| existing.and_then(|summary| summary.avatar_url.clone())),
        peer: facts
            .peer
            .or_else(|| existing.and_then(|summary| summary.peer.clone())),
        role: role(&me.role),
        state: state(&me.state),
        accepted: me.accepted,
        member_count: u32::try_from(view.member_count).unwrap_or_default(),
        last_seq: existing.map(|summary| summary.last_seq).unwrap_or_default(),
        last_at: existing.map_or(view.last_at, |summary| summary.last_at.max(view.last_at)),
        last_read_seq,
        unread: existing.map(|summary| summary.unread).unwrap_or_default(),
        mentions: existing.map(|summary| summary.mentions).unwrap_or_default(),
        preview: existing.and_then(|summary| summary.preview.clone()),
        muted_until: me.muted_until,
        notify: notify(&me.notify),
        pinned_at: me.pinned_at,
        archived: me.archived,
        ttl_s: view.ttl_s,
        room_code: view.room_ref.clone(),
        history_locked: facts.history_locked,
        has_draft: existing.is_some_and(|summary| summary.has_draft),
    }
}

pub fn absorb(summary: &mut ConversationSummary, messages: &[Message]) {
    let floor = summary.last_seq;
    for message in messages {
        let Some(seq) = message.seq else {
            continue;
        };
        summary.last_at = summary.last_at.max(message.ts);
        if seq > floor && counts(message) && seq > summary.last_read_seq {
            summary.unread += 1;
            summary.mentions += u32::from(message.mentions_me);
        }
        if summary
            .preview
            .as_ref()
            .is_none_or(|preview| preview.seq <= seq)
        {
            summary.preview = Some(preview(message, seq));
        }
    }
    let newest = messages.iter().filter_map(|message| message.seq).max();
    summary.last_seq = summary.last_seq.max(newest.unwrap_or_default());
}

pub fn recount(summary: &mut ConversationSummary, unread: &[Message]) {
    let counted: Vec<&Message> = unread
        .iter()
        .filter(|message| message.seq.is_some_and(|seq| seq > summary.last_read_seq))
        .filter(|message| counts(message))
        .collect();
    summary.unread = u32::try_from(counted.len()).unwrap_or(u32::MAX);
    summary.mentions = u32::try_from(counted.iter().filter(|message| message.mentions_me).count())
        .unwrap_or(u32::MAX);
}

fn counts(message: &Message) -> bool {
    !message.mine
        && matches!(
            message.body,
            MessageBody::Text(_) | MessageBody::Locked | MessageBody::Unknown
        )
}

pub fn preview(message: &Message, seq: i64) -> MessagePreview {
    let (kind, text) = match &message.body {
        MessageBody::Text(rich) if rich.text.trim().is_empty() && !message.embeds.is_empty() => {
            (PreviewKind::Embed, message.embeds[0].title.clone())
        }
        MessageBody::Text(rich) => (PreviewKind::Text, rich.text.clone()),
        MessageBody::System(event) if event.kind == SystemKind::Deleted => {
            (PreviewKind::Deleted, String::new())
        }
        MessageBody::System(_) => (PreviewKind::System, String::new()),
        MessageBody::Deleted => (PreviewKind::Deleted, String::new()),
        MessageBody::Locked => (PreviewKind::Locked, String::new()),
        MessageBody::Unknown => (PreviewKind::Unknown, String::new()),
    };
    MessagePreview {
        seq,
        sender: message.sender.clone(),
        kind,
        text: text.chars().take(PREVIEW_CHARS).collect(),
        ts: message.ts,
        expires_at: message.expires_at,
    }
}

fn role(raw: &str) -> MemberRole {
    match raw {
        "owner" => MemberRole::Owner,
        "admin" => MemberRole::Admin,
        _ => MemberRole::Member,
    }
}

fn state(raw: &str) -> MemberState {
    match raw {
        "invited" => MemberState::Invited,
        "left" => MemberState::Left,
        "kicked" => MemberState::Kicked,
        "banned" => MemberState::Banned,
        _ => MemberState::Active,
    }
}

fn notify(raw: &str) -> NotifyLevel {
    match raw {
        "mentions" => NotifyLevel::Mentions,
        "none" => NotifyLevel::None,
        _ => NotifyLevel::All,
    }
}
