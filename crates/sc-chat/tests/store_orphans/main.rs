#[path = "../store_support/mod.rs"]
mod store_support;

mod aside;
mod files;

use std::path::{Path, PathBuf};

use sc_chat::store::Store;
use sc_chat::{KeyName, KeyStore};
use store_support::{MemoryKeys, PEER, TempDir, conv, post};

fn messages(store: &Store) -> i64 {
    store
        .conn()
        .unwrap()
        .query_row("SELECT count(*) FROM messages", [], |row| row.get(0))
        .unwrap()
}

fn orphans(dir: &TempDir) -> Vec<PathBuf> {
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name()?.to_str()?.to_owned();
            let at = name.strip_prefix("chat.db.orphan-")?.parse().ok()?;
            Some((at, path))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
}

fn names(dir: &TempDir) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn parked(dir: &TempDir, suffix: &str) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|name| name.starts_with("chat.db.orphan-") && name.ends_with(suffix))
        .collect()
}

fn master(keys: &MemoryKeys) -> [u8; 32] {
    let saved = keys.load(KeyName::Database).unwrap().unwrap();
    saved.as_slice().try_into().unwrap()
}

fn lose_key_after(path: &Path, keys: &MemoryKeys, text: &str) -> [u8; 32] {
    let store = Store::open_keyed(path, keys).unwrap();
    store.put_messages(&[post(conv(), 1, PEER, text)]).unwrap();
    drop(store);
    let lost = master(keys);
    keys.delete(KeyName::Database).unwrap();
    lost
}
