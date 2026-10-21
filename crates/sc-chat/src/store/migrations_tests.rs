use rusqlite::Connection;

use super::{V1, VERSION, migrate, prepare, version};

const TABLES: [&str; 12] = [
    "meta",
    "users",
    "conversations",
    "members",
    "marks",
    "messages",
    "search",
    "message_embeds",
    "conversation_keys",
    "drafts",
    "outbox",
    "hidden_messages",
];

fn fresh() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    prepare(&conn).unwrap();
    migrate(&conn).unwrap();
    conn
}

#[test]
fn every_table_exists_after_migrating() {
    let conn = fresh();
    for table in TABLES {
        let found: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 1, "{table}");
    }
    assert_eq!(version(&conn).unwrap(), VERSION);
}

#[test]
fn migrate_twice_is_a_no_op() {
    let conn = fresh();
    migrate(&conn).unwrap();
    assert_eq!(version(&conn).unwrap(), VERSION);
}

#[test]
fn every_connection_deletes_securely() {
    let conn = fresh();
    let secure: i64 = conn
        .pragma_query_value(None, "secure_delete", |row| row.get(0))
        .unwrap();
    assert_eq!(secure, 1);
}

#[test]
fn search_drops_an_entry_by_rowid() {
    let conn = fresh();
    conn.execute(
        "INSERT INTO search (rowid, tokens) VALUES (7, 'a1b2c3 d4e5f6')",
        [],
    )
    .unwrap();
    let hits = |conn: &Connection| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM search WHERE search MATCH 'a1b2c3'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(hits(&conn), 1);
    conn.execute("DELETE FROM search WHERE rowid = 7", [])
        .unwrap();
    assert_eq!(hits(&conn), 0);
}

#[test]
fn a_message_seq_is_unique_per_conversation() {
    let conn = fresh();
    let insert = "INSERT INTO messages (conversation_id, seq, client_id, sender, class, epoch, ts, content) VALUES (x'01', 1, x'02', 'u', 'post', 0, 0, 'open')";
    conn.execute(insert, []).unwrap();
    assert!(conn.execute(insert, []).is_err());
}

#[test]
fn a_v1_store_moves_to_v2_and_keeps_its_rows() {
    let conn = Connection::open_in_memory().unwrap();
    prepare(&conn).unwrap();
    conn.execute_batch(V1).unwrap();
    conn.pragma_update(None, "user_version", 1).unwrap();
    conn.execute("INSERT INTO meta (key, value) VALUES ('me', x'31')", [])
        .unwrap();
    migrate(&conn).unwrap();
    assert_eq!(version(&conn).unwrap(), VERSION);
    let kept: i64 = conn
        .query_row("SELECT count(*) FROM meta WHERE key = 'me'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(kept, 1);
    conn.execute(
        "INSERT INTO hidden_messages (conversation_id, seq) VALUES (x'01', 4)",
        [],
    )
    .unwrap();
}
