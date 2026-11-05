use std::collections::BTreeMap;
use std::fs::DirEntry;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};

use super::dbkey::sibling;
use crate::contracts::{ChatError, ChatResult};

pub const ORPHANS_KEPT: usize = 2;

const ORPHAN: &str = ".orphan-";
const COMPANIONS: [&str; 2] = ["-wal", "-shm"];

#[derive(Default)]
struct Orphan {
    database: bool,
    files: Vec<PathBuf>,
}

pub fn set_aside(path: &Path) -> ChatResult<()> {
    if !holds_chats(path) {
        return remove_database(path);
    }
    let orphan = next_orphan(path);
    let mut moved = Vec::new();
    for suffix in ["", COMPANIONS[0], COMPANIONS[1]] {
        let (from, to) = (sibling(path, suffix), sibling(&orphan, suffix));
        match std::fs::rename(&from, &to) {
            Ok(()) => moved.push((from, to)),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => {
                put_back(&moved);
                return Err(ChatError::Store);
            }
        }
    }
    prune(path);
    Ok(())
}

pub fn set_strays_aside(path: &Path) -> ChatResult<()> {
    if present(path)? {
        return Ok(());
    }
    let mut strays = Vec::new();
    for suffix in COMPANIONS {
        if present(&sibling(path, suffix))? {
            strays.push(suffix);
        }
    }
    if strays.is_empty() {
        return Ok(());
    }
    let parked = next_orphan(path);
    for suffix in strays {
        std::fs::rename(sibling(path, suffix), sibling(&parked, suffix))
            .map_err(|_| ChatError::Store)?;
    }
    prune(path);
    Ok(())
}

pub fn remove_database(path: &Path) -> ChatResult<()> {
    remove_all([COMPANIONS[0], COMPANIONS[1], ""].map(|suffix| sibling(path, suffix)))
}

pub fn remove_orphans(path: &Path) -> ChatResult<()> {
    remove_all(
        orphans(path)
            .into_iter()
            .flat_map(|(_, orphan)| orphan.files),
    )
}

fn put_back(moved: &[(PathBuf, PathBuf)]) {
    for (from, to) in moved.iter().rev() {
        if std::fs::rename(to, from).is_err() {
            return;
        }
    }
}

fn prune(path: &Path) {
    let (databases, strays): (Vec<_>, Vec<_>) = orphans(path)
        .into_iter()
        .partition(|(_, orphan)| orphan.database);
    let stale = databases
        .into_iter()
        .skip(ORPHANS_KEPT)
        .chain(strays.into_iter().skip(ORPHANS_KEPT));
    let _ = remove_all(stale.flat_map(|(_, orphan)| orphan.files));
}

fn remove_all(files: impl IntoIterator<Item = PathBuf>) -> ChatResult<()> {
    let mut failed = false;
    for file in files {
        match std::fs::remove_file(&file) {
            Err(error) if error.kind() != ErrorKind::NotFound => failed = true,
            _ => {}
        }
    }
    if failed {
        Err(ChatError::Store)
    } else {
        Ok(())
    }
}

fn present(path: &Path) -> ChatResult<bool> {
    path.try_exists().map_err(|_| ChatError::Store)
}

fn holds_chats(path: &Path) -> bool {
    if !path.try_exists().unwrap_or(true) {
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

fn next_orphan(path: &Path) -> PathBuf {
    let after_newest = orphans(path)
        .first()
        .map_or(0, |(at, _)| at.saturating_add(1));
    sibling(path, &format!("{ORPHAN}{}", unix_now().max(after_newest)))
}

fn orphans(path: &Path) -> Vec<(u64, Orphan)> {
    let Some(prefix) = orphan_prefix(path) else {
        return Vec::new();
    };
    let mut found: BTreeMap<u64, Orphan> = BTreeMap::new();
    for entry in std::fs::read_dir(folder(path))
        .into_iter()
        .flatten()
        .flatten()
    {
        if let Some((at, suffix)) = orphan_name(&entry, &prefix) {
            let orphan = found.entry(at).or_default();
            orphan.database |= suffix.is_empty();
            orphan.files.push(entry.path());
        }
    }
    found.into_iter().rev().collect()
}

fn orphan_name(entry: &DirEntry, prefix: &str) -> Option<(u64, &'static str)> {
    if !entry.file_type().ok()?.is_file() {
        return None;
    }
    let name = entry.file_name();
    let rest = name.to_str()?.strip_prefix(prefix)?;
    let (digits, suffix) = COMPANIONS
        .into_iter()
        .find_map(|suffix| Some((rest.strip_suffix(suffix)?, suffix)))
        .unwrap_or((rest, ""));
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((digits.parse().ok()?, suffix))
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

#[cfg(test)]
#[path = "orphan_tests.rs"]
mod tests;
