use crate::api::wire::{SyncAnswer, SyncRequest};
use crate::api::{ANSWER_SLACK_MS, Api, CallOptions, decode, header, inner_call};
use crate::contracts::{ChatResult, ConvId, Tier};

pub const RETRY_AFTER_MS: &str = "retry-after-ms";

const NO_CONTENT: u16 = 204;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncReply {
    Answer(Box<SyncAnswer>),
    Idle,
    Later { retry_ms: u64 },
}

impl Api {
    pub async fn sync(&self, request: &SyncRequest, tier: Tier) -> ChatResult<SyncReply> {
        let call = inner_call("POST", "/chat/sync", "", Some(request))?;
        let answer = self
            .call(call, Self::sync_options(tier, request.hold_ms))
            .await?;
        if answer.status != NO_CONTENT {
            return Ok(SyncReply::Answer(Box::new(decode(&answer)?)));
        }
        let later = header(&answer, RETRY_AFTER_MS).and_then(|value| value.parse().ok());
        Ok(match later {
            Some(retry_ms) => SyncReply::Later { retry_ms },
            None => SyncReply::Idle,
        })
    }

    pub async fn typing(&self, conversation_id: ConvId) -> ChatResult<()> {
        self.post_empty(&format!("/chat/conversations/{conversation_id}/typing"))
            .await
    }

    pub fn sync_options(tier: Tier, hold_ms: u64) -> CallOptions {
        CallOptions {
            tier: Some(tier),
            timeout_ms: hold_ms + ANSWER_SLACK_MS,
            long_poll: hold_ms > 0,
        }
    }
}
