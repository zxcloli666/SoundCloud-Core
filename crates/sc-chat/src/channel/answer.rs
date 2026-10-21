use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::InnerResponse;
use serde_json::Value;

use super::{Channel, Sealed};
use crate::contracts::{ChatError, ChatResult, TransportResponse};

pub const BUSY_RETRY_MS: u64 = 1_000;
pub const BUSY_RETRY_MAX_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerAnswer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl InnerAnswer {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn error(&self) -> Option<ChatError> {
        if self.is_success() {
            return None;
        }
        let body = serde_json::from_slice(&self.body).unwrap_or(Value::Null);
        Some(ChatError::from_server(&body))
    }
}

impl Sealed {
    pub fn open(&self, body: &[u8]) -> ChatResult<InnerAnswer> {
        let plain = self.response_key.open(&self.enc, body)?;
        let answer = InnerResponse::decode(plain.as_slice()).map_err(|_| ChatError::Protocol)?;
        Ok(InnerAnswer {
            status: u16::try_from(answer.status).map_err(|_| ChatError::Protocol)?,
            headers: answer
                .headers
                .into_iter()
                .map(|header| (header.name, header.value))
                .collect(),
            body: answer.body,
        })
    }
}

impl Channel {
    pub fn receive(
        &self,
        sealed: &Sealed,
        response: &TransportResponse,
    ) -> ChatResult<InnerAnswer> {
        let answer = match response.status {
            200 => sealed.open(&response.body)?,
            503 => {
                let retry_ms = response.retry_after_ms.unwrap_or(BUSY_RETRY_MS);
                return Err(ChatError::Busy {
                    retry_ms: retry_ms.clamp(BUSY_RETRY_MS, BUSY_RETRY_MAX_MS),
                });
            }
            400 => return Err(ChatError::Protocol),
            413 => return Err(ChatError::Invalid),
            _ => return Err(ChatError::Offline),
        };
        if let Some(ChatError::Clock { server_now }) = answer.error()
            && server_now > 0
        {
            self.note_server_now(server_now);
        }
        Ok(answer)
    }
}
