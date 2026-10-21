use rusqlite::Connection;

use super::tokens;
use crate::contracts::ChatResult;

pub fn replace(
    conn: &Connection,
    key: &[u8; 32],
    rowid: i64,
    text: Option<&str>,
) -> ChatResult<()> {
    remove(conn, rowid)?;
    insert(conn, key, rowid, text)
}

pub fn insert(conn: &Connection, key: &[u8; 32], rowid: i64, text: Option<&str>) -> ChatResult<()> {
    let Some(text) = text else {
        return Ok(());
    };
    let document = tokens::document(key, text)?;
    if document.is_empty() {
        return Ok(());
    }
    conn.prepare_cached("INSERT INTO search (rowid, tokens) VALUES (?1, ?2)")?
        .execute(rusqlite::params![rowid, document])?;
    Ok(())
}

pub fn remove(conn: &Connection, rowid: i64) -> ChatResult<()> {
    conn.prepare_cached("DELETE FROM search WHERE rowid = ?1")?
        .execute([rowid])?;
    Ok(())
}
