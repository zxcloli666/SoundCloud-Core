use sc_chat_wire::proto;

use crate::contracts::{ConversationMeta, Embed, EmbedKind, EmbedTrack, RoomInvite};

pub fn embed_to_wire(embed: &Embed) -> proto::Embed {
    proto::Embed {
        kind: kind_to_wire(embed.kind).into(),
        urn: embed.urn.clone(),
        permalink: embed.permalink.clone(),
        secret_token: embed.secret_token.clone().unwrap_or_default(),
        title: embed.title.clone(),
        artist: embed.artist.clone().unwrap_or_default(),
        artwork_url: embed.artwork_url.clone().unwrap_or_default(),
        duration_ms: embed.duration_ms.unwrap_or_default(),
        access: embed.access.clone().unwrap_or_default(),
        at_ms: embed.at_ms.unwrap_or_default(),
        tracks: embed.tracks.iter().map(track_to_wire).collect(),
        room: embed.room.as_ref().map(room_to_wire),
    }
}

pub fn embed_from_wire(embed: proto::Embed) -> Option<Embed> {
    let kind = kind_from_wire(proto::EmbedKind::try_from(embed.kind).ok()?)?;
    Some(Embed {
        kind,
        urn: embed.urn,
        permalink: embed.permalink,
        secret_token: filled(embed.secret_token),
        title: embed.title,
        artist: filled(embed.artist),
        artwork_url: filled(embed.artwork_url),
        duration_ms: nonzero(embed.duration_ms),
        access: filled(embed.access),
        at_ms: nonzero(embed.at_ms),
        tracks: embed.tracks.into_iter().map(track_from_wire).collect(),
        room: embed.room.map(room_from_wire),
    })
}

pub fn meta_to_wire(meta: &ConversationMeta) -> proto::Meta {
    proto::Meta {
        title: meta.title.clone(),
        description: meta.description.clone(),
        avatar_media_id: meta.avatar_media_id.clone().unwrap_or_default(),
    }
}

pub fn meta_from_wire(meta: proto::Meta) -> ConversationMeta {
    ConversationMeta {
        title: meta.title,
        description: meta.description,
        avatar_media_id: filled(meta.avatar_media_id),
    }
}

fn track_to_wire(track: &EmbedTrack) -> proto::EmbedTrack {
    proto::EmbedTrack {
        urn: track.urn.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        artwork_url: track.artwork_url.clone().unwrap_or_default(),
    }
}

fn track_from_wire(track: proto::EmbedTrack) -> EmbedTrack {
    EmbedTrack {
        urn: track.urn,
        title: track.title,
        artist: track.artist,
        duration_ms: track.duration_ms,
        artwork_url: filled(track.artwork_url),
    }
}

fn room_to_wire(room: &RoomInvite) -> proto::RoomInvite {
    proto::RoomInvite {
        code: room.code.clone(),
        title: room.title.clone().unwrap_or_default(),
        host: room.host.clone(),
    }
}

fn room_from_wire(room: proto::RoomInvite) -> RoomInvite {
    RoomInvite {
        code: room.code,
        title: filled(room.title),
        host: room.host,
    }
}

fn kind_to_wire(kind: EmbedKind) -> proto::EmbedKind {
    match kind {
        EmbedKind::Track => proto::EmbedKind::Track,
        EmbedKind::Playlist => proto::EmbedKind::Playlist,
        EmbedKind::User => proto::EmbedKind::User,
        EmbedKind::RoomInvite => proto::EmbedKind::RoomInvite,
    }
}

fn kind_from_wire(kind: proto::EmbedKind) -> Option<EmbedKind> {
    match kind {
        proto::EmbedKind::Track => Some(EmbedKind::Track),
        proto::EmbedKind::Playlist => Some(EmbedKind::Playlist),
        proto::EmbedKind::User => Some(EmbedKind::User),
        proto::EmbedKind::RoomInvite => Some(EmbedKind::RoomInvite),
        proto::EmbedKind::Unspecified => None,
    }
}

fn filled(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

fn nonzero(value: u64) -> Option<u64> {
    (value != 0).then_some(value)
}
