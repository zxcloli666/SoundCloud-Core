use serde_json::Value;

use crate::api::Api;
use crate::api::path::{query, segment};
use crate::api::wire::{
    ModeratorSwitch, MuteFor, RoomChatView, RoomPreviewView, SuggestTrack, TrackSnapshot,
};
use crate::contracts::{ChatResult, Embed, RoomModeration, RoomSettingsPatch};

impl Api {
    pub async fn room_chat(&self, code: &str) -> ChatResult<RoomChatView> {
        self.get(&room_path(code, "chat"), "").await
    }

    pub async fn room_preview(&self, code: &str) -> ChatResult<RoomPreviewView> {
        self.get(&room_path(code, "preview"), "").await
    }

    pub async fn room_moderate(&self, code: &str, action: &RoomModeration) -> ChatResult<()> {
        match action {
            RoomModeration::Kick { user_id } => self.room_remove(code, user_id, false).await,
            RoomModeration::Ban { user_id } => self.room_remove(code, user_id, true).await,
            RoomModeration::Mute { user_id, seconds } => {
                let path = room_path(code, &format!("members/{}/mute", segment(user_id)));
                self.put(&path, &MuteFor { seconds: *seconds }).await
            }
            RoomModeration::Moderator { user_id, on } => {
                let path = room_path(code, &format!("moderators/{}", segment(user_id)));
                self.put(&path, &ModeratorSwitch { on: *on }).await
            }
        }
    }

    async fn room_remove(&self, code: &str, user: &str, ban: bool) -> ChatResult<()> {
        let path = room_path(code, &format!("members/{}", segment(user)));
        let query = query(&[("ban", ban.then(|| "true".to_owned()))]);
        self.delete(&path, &query).await
    }

    pub async fn room_settings(&self, code: &str, patch: &RoomSettingsPatch) -> ChatResult<Value> {
        self.put(&room_path(code, "settings"), patch).await
    }

    pub async fn room_suggest(&self, code: &str, track: &Embed) -> ChatResult<()> {
        let body = SuggestTrack {
            track: TrackSnapshot {
                urn: track.urn.clone(),
                title: track.title.clone(),
                artist: track.artist.clone(),
                artwork_url: track.artwork_url.clone(),
                duration_ms: track.duration_ms,
            },
        };
        self.post(&room_path(code, "suggest"), &body).await
    }
}

fn room_path(code: &str, rest: &str) -> String {
    format!("/chat/rooms/{}/{rest}", segment(code))
}
