use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::dbkey::{remove_files, sibling};
use crate::contracts::{ChatError, ChatResult};

pub const ORPHANS_KEPT: usize = 2;

const ORPHAN: &str = ".orphan-";
const COMPANIONS: [&str; 2] = ["-wal", "-shm"];

pub fn set_aside(path: &Path) -> ChatResult<()> {
    if !holds_chats(path) {
        return remove_files(path);
    }
    let older = orphans(path);
    let orphan = next_orphan(path, &older);
    let mut moved = Vec::new();
    for suffix in ["", COMPANIONS[0], COMPANIONS[1]] {
        let (from, to) = (sibling(path, suffix), sibling(&orphan, suffix));
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push((from, to)),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => {
                for (from, to) in moved.iter().rev() {
                    let _ = std::fs::rename(to, from);
                }
                return Err(ChatError::Store);
            }
        }
    }
    for (_, stale) in older.into_iter().skip(ORPHANS_KEPT - 1) {
        let _ = remove_files(&stale);
    }
    Ok(())
}

pub fn set_strays_aside(path: &Path) -> ChatResult<()> {
    if path.exists() {
        return Ok(());
    }
    let older = orphans(path);
    for suffix in COMPANIONS {
        let stray = sibling(path, suffix);
        if !stray.exists() {
            continue;
        }
        let home = match older.first() {
            Some((_, newest)) if !sibling(newest, suffix).exists() => newest.clone(),
            _ => next_orphan(path, &older),
        };
        std::fs::rename(&stray, sibling(&home, suffix)).map_err(|_| ChatError::Store)?;
    }
    Ok(())
}

pub fn remove_orphans(path: &Path) -> ChatResult<()> {
    let Some(prefix) = orphan_prefix(path) else {
        return Ok(());
    };
    let mut failed = false;
    for entry in std::fs::read_dir(folder(path))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| name.starts_with(&prefix)) {
            match std::fs::remove_file(entry.path()) {
                Err(error) if error.kind() != ErrorKind::NotFound => failed = true,
                _ => {}
            }
        }
    }
    if failed {
        Err(ChatError::Store)
    } else {
        Ok(())
    }
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

fn next_orphan(path: &Path, older: &[(u64, PathBuf)]) -> PathBuf {
    let after_newest = older.first().map_or(0, |(at, _)| at + 1);
    sibling(path, &format!("{ORPHAN}{}", unix_now().max(after_newest)))
}

fn orphans(path: &Path) -> Vec<(u64, PathBuf)> {
    let Some(prefix) = orphan_prefix(path) else {
        return Vec::new();
    };
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(folder(path))
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

fn orphan_prefix(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    Some(format!("{name}{ORPHAN}"))
}

fn folder(path: &Path) -> &Path {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}
