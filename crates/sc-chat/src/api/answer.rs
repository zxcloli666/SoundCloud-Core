use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::channel::InnerAnswer;
use crate::contracts::{ChatError, ChatResult};

pub fn decode<T: DeserializeOwned>(answer: &InnerAnswer) -> ChatResult<T> {
    if let Some(error) = error_of(answer) {
        return Err(error);
    }
    let body: &[u8] = if answer.body.is_empty() {
        b"null"
    } else {
        &answer.body
    };
    serde_json::from_slice(body).map_err(|_| ChatError::Protocol)
}

pub fn error_of(answer: &InnerAnswer) -> Option<ChatError> {
    if answer.status < 400 {
        return None;
    }
    let named = serde_json::from_slice::<Value>(&answer.body)
        .ok()
        .map(|body| ChatError::from_server(&body))
        .filter(|error| *error != ChatError::Protocol);
    Some(named.unwrap_or_else(|| by_status(answer.status)))
}

pub fn header<'a>(answer: &'a InnerAnswer, name: &str) -> Option<&'a str> {
    answer
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn by_status(status: u16) -> ChatError {
    match status {
        400 | 409 | 422 => ChatError::Invalid,
        401 => ChatError::ReloginRequired,
        403 => ChatError::Unavailable,
        404 => ChatError::NotFound,
        429 | 503 => ChatError::Busy { retry_ms: 0 },
        500..=599 => ChatError::Internal,
        _ => ChatError::Protocol,
    }
}
