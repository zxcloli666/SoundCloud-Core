use sc_chat_wire::proto;

use super::{LINK_MAX, REACTION_MAX};
use crate::contracts::{ChatError, ChatResult, RichText, Span, SpanKind, TEXT_MAX};

pub fn to_wire(text: &RichText) -> ChatResult<proto::Text> {
    let length = utf16_len(&text.text);
    if length > TEXT_MAX {
        return Err(ChatError::Invalid);
    }
    let spans = text
        .spans
        .iter()
        .map(|span| span_to_wire(span, length))
        .collect::<Option<Vec<_>>>()
        .ok_or(ChatError::Invalid)?;
    Ok(proto::Text {
        text: text.text.clone(),
        spans,
    })
}

pub fn from_wire(text: proto::Text) -> ChatResult<RichText> {
    let length = utf16_len(&text.text);
    if length > TEXT_MAX {
        return Err(ChatError::Protocol);
    }
    let spans = text
        .spans
        .into_iter()
        .filter_map(|span| span_from_wire(span, length))
        .collect();
    Ok(RichText {
        text: text.text,
        spans,
    })
}

pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

pub fn is_web_link(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let web = scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http");
    let clean = !url.chars().any(|c| c.is_control() || c.is_whitespace());
    web && clean && !rest.is_empty() && url.len() <= LINK_MAX
}

pub fn is_reaction(emoji: &str) -> bool {
    let odd = emoji.chars().any(|c| c.is_control() || c.is_whitespace());
    !emoji.is_empty() && !odd && emoji.len() <= REACTION_MAX
}

fn span_to_wire(span: &Span, length: usize) -> Option<proto::Span> {
    let wire = proto::Span {
        kind: kind_to_wire(span.kind).into(),
        start: span.start,
        len: span.len,
        url: kept(span.kind == SpanKind::Link, &span.url),
        user_id: kept(span.kind == SpanKind::Mention, &span.user_id),
    };
    span_from_wire(wire.clone(), length).map(|_| wire)
}

fn span_from_wire(span: proto::Span, length: usize) -> Option<Span> {
    let kind = kind_from_wire(proto::SpanKind::try_from(span.kind).ok()?)?;
    let end = usize::try_from(span.start.checked_add(span.len)?).ok()?;
    if span.len == 0 || end > length {
        return None;
    }
    let url = Some(span.url).filter(|url| kind == SpanKind::Link && is_web_link(url));
    let user_id = Some(span.user_id).filter(|id| kind == SpanKind::Mention && !id.is_empty());
    let complete = match kind {
        SpanKind::Link => url.is_some(),
        SpanKind::Mention => user_id.is_some(),
        _ => true,
    };
    complete.then_some(Span {
        kind,
        start: span.start,
        len: span.len,
        url,
        user_id,
    })
}

fn kept(wanted: bool, value: &Option<String>) -> String {
    match wanted {
        true => value.clone().unwrap_or_default(),
        false => String::new(),
    }
}

fn kind_to_wire(kind: SpanKind) -> proto::SpanKind {
    match kind {
        SpanKind::Bold => proto::SpanKind::Bold,
        SpanKind::Italic => proto::SpanKind::Italic,
        SpanKind::Code => proto::SpanKind::Code,
        SpanKind::Spoiler => proto::SpanKind::Spoiler,
        SpanKind::Link => proto::SpanKind::Link,
        SpanKind::Mention => proto::SpanKind::Mention,
    }
}

fn kind_from_wire(kind: proto::SpanKind) -> Option<SpanKind> {
    match kind {
        proto::SpanKind::Bold => Some(SpanKind::Bold),
        proto::SpanKind::Italic => Some(SpanKind::Italic),
        proto::SpanKind::Code => Some(SpanKind::Code),
        proto::SpanKind::Spoiler => Some(SpanKind::Spoiler),
        proto::SpanKind::Link => Some(SpanKind::Link),
        proto::SpanKind::Mention => Some(SpanKind::Mention),
        proto::SpanKind::Unspecified => None,
    }
}
