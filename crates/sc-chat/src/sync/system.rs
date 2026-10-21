use serde_json::Value;

use crate::contracts::{Embed, EmbedKind, SystemEvent, SystemKind};
use crate::embeds::{clean, validate};

pub fn parse(body: &[u8]) -> SystemEvent {
    let Ok(row) = serde_json::from_slice::<Value>(body) else {
        return event(SystemKind::Other);
    };
    match (text(&row, "kind"), text(&row, "type")) {
        (Some(kind), _) => conversation_event(&row, &kind),
        (None, Some(kind)) => room_event(&row, &kind),
        _ => event(SystemKind::Other),
    }
}

fn conversation_event(row: &Value, kind: &str) -> SystemEvent {
    let mut parsed = event(match kind {
        "created" => SystemKind::Created,
        "added" => SystemKind::Added,
        "joined" => SystemKind::Joined,
        "left" => SystemKind::Left,
        "kicked" => SystemKind::Kicked,
        "banned" => SystemKind::Banned,
        "renamed" => SystemKind::Renamed,
        "ttl" => SystemKind::Ttl,
        "role" => SystemKind::Role,
        "deleted" => SystemKind::Deleted,
        _ => SystemKind::Other,
    });
    parsed.actor = text(row, "actor");
    parsed.target = text(row, "target").or_else(|| first_target(row));
    parsed.ttl_s = row["ttl_s"]
        .as_u64()
        .and_then(|ttl| u32::try_from(ttl).ok());
    parsed.role = serde_json::from_value(row["role"].clone()).ok();
    parsed
}

fn room_event(row: &Value, kind: &str) -> SystemEvent {
    let banned = row["ban"].as_bool().unwrap_or_default();
    let mut parsed = event(match kind {
        "join" => SystemKind::Joined,
        "leave" => SystemKind::Left,
        "kick" if banned => SystemKind::Banned,
        "kick" => SystemKind::Kicked,
        "host" => SystemKind::Handoff,
        "mute" => SystemKind::Muted,
        "title" => SystemKind::Renamed,
        "track" => SystemKind::Track,
        "moderator" => SystemKind::Moderator,
        "suggest" => SystemKind::Suggest,
        _ => SystemKind::Other,
    });
    match kind {
        "join" | "leave" | "suggest" => parsed.actor = text(row, "user_id"),
        "title" => parsed.actor = text(row, "by"),
        "kick" | "mute" | "moderator" => {
            parsed.actor = text(row, "by");
            parsed.target = text(row, "user_id");
        }
        "host" => parsed.target = text(row, "user_id"),
        _ => {}
    }
    parsed.name = match kind {
        "title" => shown(row, "title"),
        _ => shown(row, "name"),
    };
    parsed.until = row["until_ms"].as_i64();
    parsed.on = row["on"].as_bool();
    parsed.track = match kind {
        "track" => track(row),
        "suggest" => track(&row["track"]),
        _ => None,
    }
    .map(Box::new);
    parsed
}

fn track(row: &Value) -> Option<Embed> {
    let snapshot = Embed {
        kind: EmbedKind::Track,
        urn: text(row, "urn")?,
        permalink: String::new(),
        secret_token: None,
        title: text(row, "title")?,
        artist: text(row, "artist"),
        artwork_url: text(row, "artwork_url"),
        duration_ms: row["duration_ms"].as_u64(),
        access: None,
        at_ms: None,
        tracks: Vec::new(),
        room: None,
    };
    validate(&snapshot).ok()
}

fn first_target(row: &Value) -> Option<String> {
    row["targets"]
        .as_array()?
        .first()?
        .as_str()
        .map(str::to_owned)
}

fn text(row: &Value, field: &str) -> Option<String> {
    row[field].as_str().map(str::to_owned)
}

fn shown(row: &Value, field: &str) -> Option<String> {
    clean::optional_text(row[field].as_str(), clean::TEXT_MAX)
}

fn event(kind: SystemKind) -> SystemEvent {
    SystemEvent {
        kind,
        actor: None,
        target: None,
        name: None,
        ttl_s: None,
        until: None,
        track: None,
        role: None,
        on: None,
    }
}
