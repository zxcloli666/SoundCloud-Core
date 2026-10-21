pub(crate) mod clean;
mod link;
pub mod wire;

use crate::contracts::{
    ChatError, ChatResult, EMBED_BYTES_MAX, EMBEDS_MAX, Embed, EmbedKind, EmbedTrack, RoomInvite,
};

pub const TRACKS_MAX: usize = 5;
pub const DURATION_MAX_MS: u64 = 7 * 24 * 3_600_000;

pub fn validate(embed: &Embed) -> ChatResult<Embed> {
    let room_invite = embed.kind == EmbedKind::RoomInvite;
    let urn = match clean::urn(&embed.urn) {
        Some(urn) => urn,
        None if room_invite => String::new(),
        None => return Err(ChatError::Invalid),
    };
    let title = clean::text(&embed.title, clean::TEXT_MAX);
    if title.is_empty() && !room_invite {
        return Err(ChatError::Invalid);
    }
    let duration_ms = embed
        .duration_ms
        .filter(|ms| (1..=DURATION_MAX_MS).contains(ms));
    let room = match (room_invite, &embed.room) {
        (true, Some(room)) => Some(room_invite_of(room)?),
        (true, None) => return Err(ChatError::Invalid),
        (false, _) => None,
    };
    let tracks = match embed.kind {
        EmbedKind::Playlist => embed
            .tracks
            .iter()
            .filter_map(track)
            .take(TRACKS_MAX)
            .collect(),
        _ => Vec::new(),
    };
    let cleaned = Embed {
        kind: embed.kind,
        urn,
        permalink: link::soundcloud(&embed.permalink).unwrap_or_default(),
        secret_token: clean::secret(embed.secret_token.as_deref()),
        title,
        artist: clean::optional_text(embed.artist.as_deref(), clean::TEXT_MAX),
        artwork_url: link::artwork(embed.artwork_url.as_deref()),
        duration_ms,
        access: clean::access(embed.access.as_deref()),
        at_ms: embed
            .at_ms
            .filter(|at| *at < duration_ms.unwrap_or(DURATION_MAX_MS)),
        tracks,
        room,
    };
    fit(cleaned)
}

pub fn validate_all(embeds: &[Embed]) -> Vec<Embed> {
    embeds
        .iter()
        .filter_map(|embed| validate(embed).ok())
        .take(EMBEDS_MAX)
        .collect()
}

fn track(track: &EmbedTrack) -> Option<EmbedTrack> {
    let title = clean::text(&track.title, clean::TEXT_MAX);
    if title.is_empty() {
        return None;
    }
    Some(EmbedTrack {
        urn: clean::urn(&track.urn)?,
        title,
        artist: clean::text(&track.artist, clean::TEXT_MAX),
        duration_ms: if track.duration_ms <= DURATION_MAX_MS {
            track.duration_ms
        } else {
            0
        },
        artwork_url: link::artwork(track.artwork_url.as_deref()),
    })
}

fn room_invite_of(room: &RoomInvite) -> ChatResult<RoomInvite> {
    Ok(RoomInvite {
        code: clean::room_code(&room.code).ok_or(ChatError::Invalid)?,
        title: clean::optional_text(room.title.as_deref(), clean::ROOM_TITLE_MAX),
        host: clean::text(&room.host, clean::TEXT_MAX),
    })
}

fn fit(mut embed: Embed) -> ChatResult<Embed> {
    loop {
        if size(&embed)? <= EMBED_BYTES_MAX {
            return Ok(embed);
        }
        if embed.tracks.pop().is_none() {
            return Err(ChatError::Invalid);
        }
    }
}

fn size(embed: &Embed) -> ChatResult<usize> {
    serde_json::to_vec(embed)
        .map(|json| json.len())
        .map_err(|_| ChatError::Invalid)
}
