use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::contracts::{Embed, EmbedKind, RoomPreview, UserRef};
use crate::embeds::validate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomChatView {
    pub conversation_id: Uuid,
    #[serde(default)]
    pub from_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomPreviewView {
    pub code: String,
    pub title: Option<String>,
    pub host: PreviewHost,
    pub track: Option<PreviewTrack>,
    pub listeners: u32,
    #[serde(default)]
    pub capacity: u32,
    #[serde(default)]
    pub public: bool,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewHost {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewTrack {
    pub urn: String,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuteFor {
    pub seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeratorSwitch {
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestTrack {
    pub track: TrackSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSnapshot {
    pub urn: String,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<u64>,
}

impl From<&RoomPreviewView> for RoomPreview {
    fn from(view: &RoomPreviewView) -> Self {
        RoomPreview {
            code: view.code.clone(),
            title: view.title.clone(),
            host: UserRef {
                id: view.host.id.clone(),
                name: view.host.name.clone(),
                avatar_url: view.host.avatar_url.clone(),
                permalink: None,
            },
            track: view.track.as_ref().and_then(track_card),
            listeners: view.listeners,
            alive: view.alive,
        }
    }
}

fn track_card(track: &PreviewTrack) -> Option<Embed> {
    let card = Embed {
        kind: EmbedKind::Track,
        urn: track.urn.clone(),
        permalink: String::new(),
        secret_token: None,
        title: track.title.clone(),
        artist: track.artist.clone(),
        artwork_url: track.artwork_url.clone(),
        duration_ms: track.duration_ms.and_then(|ms| u64::try_from(ms).ok()),
        access: None,
        at_ms: None,
        tracks: Vec::new(),
        room: None,
    };
    validate(&card).ok()
}
