use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ids::Millis;
use super::traits::TransportError;

pub type ChatResult<T> = Result<T, ChatError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ChatError {
    #[error("not a member of the conversation")]
    NotMember,
    #[error("the conversation key epoch is stale")]
    EpochStale,
    #[error("muted until {until}")]
    Muted { until: Millis },
    #[error("slow mode, retry in {retry_ms} ms")]
    SlowMode { retry_ms: u64 },
    #[error("the user is unavailable")]
    Unavailable,
    #[error("too many message requests")]
    RequestLimit,
    #[error("sending is limited")]
    SendLimited,
    #[error("reading is limited")]
    ReadLimited,
    #[error("the server is busy, retry in {retry_ms} ms")]
    Busy { retry_ms: u64 },
    #[error("the clock is off, server time {server_now}")]
    Clock { server_now: Millis },
    #[error("the request was replayed")]
    Replay,
    #[error("this device was revoked")]
    DeviceRevoked,
    #[error("the session needs a sealed login")]
    ReloginRequired,
    #[error("the session is bound to another device")]
    SessionBoundOther,
    #[error("this device is too young for that")]
    TooYoung,
    #[error("chat is disabled")]
    Disabled,
    #[error("sending is paused")]
    SendPaused,
    #[error("the request is invalid")]
    Invalid,
    #[error("not found")]
    NotFound,
    #[error("not implemented")]
    Unimplemented,
    #[error("the server failed")]
    Internal,
    #[error("no tier reached the server")]
    Offline,
    #[error("chat is not ready")]
    NotReady,
    #[error("the local store failed")]
    Store,
    #[error("the keychain failed")]
    Keychain,
    #[error("a key or ciphertext did not open")]
    Crypto,
    #[error("the answer is malformed")]
    Protocol,
}

impl ChatError {
    pub fn from_server(body: &Value) -> Self {
        let code = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let unsigned = |field: &str| body.get(field).and_then(Value::as_u64).unwrap_or_default();
        let signed = |field: &str| body.get(field).and_then(Value::as_i64).unwrap_or_default();
        match code.strip_prefix("chat_").unwrap_or_default() {
            "not_member" => Self::NotMember,
            "epoch_stale" => Self::EpochStale,
            "muted" => Self::Muted {
                until: signed("until"),
            },
            "slow_mode" => Self::SlowMode {
                retry_ms: unsigned("retry_ms"),
            },
            "unavailable" => Self::Unavailable,
            "request_limit" => Self::RequestLimit,
            "send_limited" => Self::SendLimited,
            "read_limited" => Self::ReadLimited,
            "busy" => Self::Busy {
                retry_ms: unsigned("retry_ms"),
            },
            "clock" => Self::Clock {
                server_now: signed("server_now"),
            },
            "replay" => Self::Replay,
            "device_revoked" => Self::DeviceRevoked,
            "relogin_required" => Self::ReloginRequired,
            "session_bound_other" => Self::SessionBoundOther,
            "too_young" => Self::TooYoung,
            "disabled" => Self::Disabled,
            "send_paused" => Self::SendPaused,
            "invalid" => Self::Invalid,
            "not_found" => Self::NotFound,
            "unimplemented" => Self::Unimplemented,
            "internal" => Self::Internal,
            _ => Self::Protocol,
        }
    }

    pub fn retry_ms(&self) -> Option<u64> {
        match self {
            Self::SlowMode { retry_ms } | Self::Busy { retry_ms } => Some(*retry_ms),
            _ => None,
        }
    }
}

impl From<sc_chat_wire::WireError> for ChatError {
    fn from(error: sc_chat_wire::WireError) -> Self {
        match error {
            sc_chat_wire::WireError::TooLarge => Self::Invalid,
            sc_chat_wire::WireError::Decode | sc_chat_wire::WireError::Envelope => Self::Protocol,
            _ => Self::Crypto,
        }
    }
}

impl From<rusqlite::Error> for ChatError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Store
    }
}

impl From<TransportError> for ChatError {
    fn from(error: TransportError) -> Self {
        match error {
            TransportError::Cancelled => Self::NotReady,
            TransportError::Timeout | TransportError::Unreachable | TransportError::Other(_) => {
                Self::Offline
            }
        }
    }
}
