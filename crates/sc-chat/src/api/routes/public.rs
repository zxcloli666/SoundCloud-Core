use uuid::Uuid;

use crate::api::Api;
use crate::api::path::{query, segment};
use crate::api::wire::{
    LogPage, ModeratorSwitch, Muted, PublicBan, PublicMute, PublicView, SlowMode,
};
use crate::contracts::{
    ChatError, ChatResult, PUBLIC_MUTE_SECONDS, PUBLIC_REASON_CHARS_MAX, PUBLIC_SLOW_MODE_MAX_S,
};

impl Api {
    pub async fn open_public(&self) -> ChatResult<PublicView> {
        self.post_empty("/chat/public/open").await
    }

    pub async fn public_watch(&self) -> ChatResult<()> {
        self.post_empty("/chat/public/watch").await
    }

    pub async fn public_unwatch(&self) -> ChatResult<()> {
        self.delete("/chat/public/watch", "").await
    }

    pub async fn public_moderator(&self, user: &str, on: bool) -> ChatResult<()> {
        self.put(&user_path("moderators", user), &ModeratorSwitch { on })
            .await
    }

    pub async fn public_mute(
        &self,
        user: &str,
        seconds: u32,
        reason: Option<&str>,
    ) -> ChatResult<Muted> {
        if !PUBLIC_MUTE_SECONDS.contains(&seconds) {
            return Err(ChatError::Invalid);
        }
        let body = PublicMute {
            seconds,
            reason: reason_of(reason)?,
        };
        self.put(&user_path("mutes", user), &body).await
    }

    pub async fn public_unmute(&self, user: &str) -> ChatResult<()> {
        self.delete(&user_path("mutes", user), "").await
    }

    pub async fn public_ban(&self, user: &str, reason: Option<&str>) -> ChatResult<()> {
        let body = PublicBan {
            reason: reason_of(reason)?,
        };
        self.put(&user_path("bans", user), &body).await
    }

    pub async fn public_unban(&self, user: &str) -> ChatResult<()> {
        self.delete(&user_path("bans", user), "").await
    }

    pub async fn public_slow_mode(&self, seconds: u32) -> ChatResult<()> {
        if seconds > PUBLIC_SLOW_MODE_MAX_S {
            return Err(ChatError::Invalid);
        }
        self.put("/chat/public/slow-mode", &SlowMode { seconds })
            .await
    }

    pub async fn public_log(
        &self,
        before: Option<Uuid>,
        limit: Option<u32>,
    ) -> ChatResult<LogPage> {
        let query = query(&[
            ("before", before.map(|id| id.to_string())),
            ("limit", limit.map(|limit| limit.to_string())),
        ]);
        self.get("/chat/public/log", &query).await
    }
}

fn user_path(kind: &str, user: &str) -> String {
    format!("/chat/public/{kind}/{}", segment(user))
}

fn reason_of(raw: Option<&str>) -> ChatResult<Option<String>> {
    let reason = raw.map(str::trim).filter(|text| !text.is_empty());
    match reason {
        Some(text) if text.chars().count() > PUBLIC_REASON_CHARS_MAX => Err(ChatError::Invalid),
        other => Ok(other.map(str::to_owned)),
    }
}
