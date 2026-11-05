use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::dbkey::{remove_files, sibling};
use crate::contracts::{ChatError, ChatResult};

pub const ORPHANS_KEPT: usize = 2;

const ORPHAN: &str = ".orphan-";

pub fn set_aside(path: &Path) -> ChatResult<()> {
    if !holds_chats(path) {
        return remove_files(path);
    }
    let older = orphans(path);
    let after_newest = older.first().map_or(0, |(at, _)| at + 1);
    let orphan = sibling(path, &format!("{ORPHAN}{}", unix_now().max(after_newest)));
    for suffix in ["", "-wal", "-shm"] {
        match std::fs::rename(sibling(path, suffix), sibling(&orphan, suffix)) {
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(ChatError::Store),
            _ => {}
        }
    }
    for (_, stale) in older.into_iter().skip(ORPHANS_KEPT - 1) {
        remove_files(&stale)?;
    }
    Ok(())
}

fn holds_chats(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return true;
    };
    let tables = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('conversations', 'messages')",
        [],
        |row| row.get::<_, i64>(0),
    );
    match tables {
        Ok(0) => false,
        Ok(2) => conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM conversations) OR EXISTS (SELECT 1 FROM messages)",
                [],
                |row| row.get(0),
            )
            .unwrap_or(true),
        _ => true,
    }
}

fn orphans(path: &Path) -> Vec<(u64, PathBuf)> {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    let prefix = format!("{name}{ORPHAN}");
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name();
            let at = file.to_str()?.strip_prefix(&prefix)?.parse().ok()?;
            Some((at, entry.path()))
        })
        .collect();
    found.sort_unstable_by_key(|(at, _)| std::cmp::Reverse(*at));
    found
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
