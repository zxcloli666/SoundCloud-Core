use serde::{Deserialize, Serialize};

use super::ids::{Millis, Seq, UserId};

pub const EMBED_BYTES_MAX: usize = 3 * 1024;
pub const EMBED_ARTWORK_HOST_SUFFIX: &str = ".sndcdn.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbedKind {
    Track,
    Playlist,
    User,
    RoomInvite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedTrack {
    pub urn: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInvite {
    pub code: String,
    pub title: Option<String>,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Embed {
    pub kind: EmbedKind,
    pub urn: String,
    pub permalink: String,
    pub secret_token: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub duration_ms: Option<u64>,
    pub access: Option<String>,
    pub at_ms: Option<u64>,
    pub tracks: Vec<EmbedTrack>,
    pub room: Option<RoomInvite>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedsQuery {
    pub kinds: Vec<EmbedKind>,
    pub before: Option<Seq>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedHit {
    pub seq: Seq,
    pub ts: Millis,
    pub sender: UserId,
    pub embed: Embed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedPage {
    pub items: Vec<EmbedHit>,
    pub more: bool,
}
