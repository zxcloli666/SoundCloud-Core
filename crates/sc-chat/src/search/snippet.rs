use super::{PREFIX_MAX, tokens};
use crate::contracts::{Message, MessageBody};

const SNIPPET_CHARS: usize = 96;
const SNIPPET_LEAD: usize = 24;

pub fn around(message: &Message, query: &str) -> String {
    let text = match &message.body {
        MessageBody::Text(text) if !text.text.trim().is_empty() => text.text.clone(),
        _ => message
            .embeds
            .first()
            .map(|embed| embed.title.clone())
            .unwrap_or_default(),
    };
    let chars: Vec<char> = text.chars().map(flatten).collect();
    let at = first_match(&chars, query).unwrap_or_default();
    let from = at.saturating_sub(SNIPPET_LEAD);
    let to = (from + SNIPPET_CHARS).min(chars.len());
    let mut snippet = String::new();
    if from > 0 {
        snippet.push('…');
    }
    snippet.extend(&chars[from..to]);
    if to < chars.len() {
        snippet.push('…');
    }
    snippet
}

fn first_match(chars: &[char], query: &str) -> Option<usize> {
    let wanted: Vec<String> = tokens::words(query)
        .into_iter()
        .map(|word| word.chars().take(PREFIX_MAX).collect())
        .collect();
    let mut start = None;
    for index in 0..=chars.len() {
        let inside = chars.get(index).is_some_and(|ch| ch.is_alphanumeric());
        match (inside, start) {
            (true, None) => start = Some(index),
            (false, Some(from)) => {
                let word = tokens::normalize(&chars[from..index].iter().collect::<String>());
                if wanted
                    .iter()
                    .any(|prefix| word.starts_with(prefix.as_str()))
                {
                    return Some(from);
                }
                start = None;
            }
            _ => {}
        }
    }
    None
}

fn flatten(ch: char) -> char {
    if ch.is_whitespace() { ' ' } else { ch }
}
