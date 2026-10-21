use sc_chat_wire::proto::{self, envelope::Body};

use super::validate;
use crate::contracts::{EMBEDS_MAX, Embed, EmbedKind, EmbedTrack, RoomInvite};

pub fn sanitize(envelope: &mut proto::Envelope) {
    envelope.embeds = std::mem::take(&mut envelope.embeds)
        .iter()
        .filter_map(checked)
        .take(EMBEDS_MAX)
        .map(|embed| to_proto(&embed))
        .collect();
    if let Some(Body::Suggest(suggest)) = &mut envelope.body {
        suggest.track = suggest
            .track
            .take()
            .and_then(|track| checked(&track))
            .filter(|embed| embed.kind == EmbedKind::Track)
            .map(|embed| Box::new(to_proto(&embed)));
    }
}

fn checked(embed: &proto::Embed) -> Option<Embed> {
    validate(&from_proto(embed)?).ok()
}

pub fn from_proto(embed: &proto::Embed) -> Option<Embed> {
    let kind = match proto::EmbedKind::try_from(embed.kind).ok()? {
        proto::EmbedKind::Track => EmbedKind::Track,
        proto::EmbedKind::Playlist => EmbedKind::Playlist,
        proto::EmbedKind::User => EmbedKind::User,
        proto::EmbedKind::RoomInvite => EmbedKind::RoomInvite,
        proto::EmbedKind::Unspecified => return None,
    };
    Some(Embed {
        kind,
        urn: embed.urn.clone(),
        permalink: embed.permalink.clone(),
        secret_token: filled(&embed.secret_token),
        title: embed.title.clone(),
        artist: filled(&embed.artist),
        artwork_url: filled(&embed.artwork_url),
        duration_ms: nonzero(embed.duration_ms),
        access: filled(&embed.access),
        at_ms: nonzero(embed.at_ms),
        tracks: embed.tracks.iter().map(track_from_proto).collect(),
        room: embed.room.as_ref().map(|room| RoomInvite {
            code: room.code.clone(),
            title: filled(&room.title),
            host: room.host.clone(),
        }),
    })
}

pub fn to_proto(embed: &Embed) -> proto::Embed {
    let kind = match embed.kind {
        EmbedKind::Track => proto::EmbedKind::Track,
        EmbedKind::Playlist => proto::EmbedKind::Playlist,
        EmbedKind::User => proto::EmbedKind::User,
        EmbedKind::RoomInvite => proto::EmbedKind::RoomInvite,
    };
    proto::Embed {
        kind: kind.into(),
        urn: embed.urn.clone(),
        permalink: embed.permalink.clone(),
        secret_token: embed.secret_token.clone().unwrap_or_default(),
        title: embed.title.clone(),
        artist: embed.artist.clone().unwrap_or_default(),
        artwork_url: embed.artwork_url.clone().unwrap_or_default(),
        duration_ms: embed.duration_ms.unwrap_or_default(),
        access: embed.access.clone().unwrap_or_default(),
        at_ms: embed.at_ms.unwrap_or_default(),
        tracks: embed.tracks.iter().map(track_to_proto).collect(),
        room: embed.room.as_ref().map(|room| proto::RoomInvite {
            code: room.code.clone(),
            title: room.title.clone().unwrap_or_default(),
            host: room.host.clone(),
        }),
    }
}

fn track_from_proto(track: &proto::EmbedTrack) -> EmbedTrack {
    EmbedTrack {
        urn: track.urn.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        artwork_url: filled(&track.artwork_url),
    }
}

fn track_to_proto(track: &EmbedTrack) -> proto::EmbedTrack {
    proto::EmbedTrack {
        urn: track.urn.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        artwork_url: track.artwork_url.clone().unwrap_or_default(),
    }
}

fn filled(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

fn nonzero(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}
