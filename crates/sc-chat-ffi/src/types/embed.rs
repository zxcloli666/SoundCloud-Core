use sc_chat::{Embed, EmbedKind, EmbedTrack, RoomInvite};

#[uniffi::remote(Enum)]
pub enum EmbedKind {
    Track,
    Playlist,
    User,
    RoomInvite,
}

#[uniffi::remote(Record)]
pub struct EmbedTrack {
    pub urn: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub artwork_url: Option<String>,
}

#[uniffi::remote(Record)]
pub struct RoomInvite {
    pub code: String,
    pub title: Option<String>,
    pub host: String,
}

#[uniffi::remote(Record)]
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
