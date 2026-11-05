use rusqlite::Connection;

use super::{V1, V2, V3, V4, V5, VERSION, migrate, prepare, version};

#[test]
fn a_v3_upload_queue_moves_to_v4_as_images() {
    let conn = Connection::open_in_memory().unwrap();
    prepare(&conn).unwrap();
    for step in [V1, V2, V3] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "user_version", 3).unwrap();
    conn.execute(
        "INSERT INTO media_uploads (id, album, position, conversation_id, image_ref, thumb_ref, state, created_at, updated_at, nonce, attachment) VALUES (x'01', x'02', 0, x'03', 'a.img', 'a.thumb', 'uploading', 1, 2, x'04', x'05')",
        [],
    )
    .unwrap();
    migrate(&conn).unwrap();
    assert_eq!(version(&conn).unwrap(), VERSION);
    let kept: (String, String, Option<String>) = conn
        .query_row(
            "SELECT kind, state, thumb_ref FROM media_uploads WHERE id = x'01'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        kept,
        ("image".into(), "uploading".into(), Some("a.thumb".into()))
    );
    conn.execute(
        "INSERT INTO media_uploads (id, album, position, conversation_id, kind, image_ref, created_at, updated_at, nonce, attachment) VALUES (x'11', x'12', 0, x'03', 'avatar', 'b.img', 1, 1, x'04', x'05')",
        [],
    )
    .unwrap();
    assert!(
        conn.execute(
            "INSERT INTO media_uploads (id, album, position, conversation_id, kind, image_ref, created_at, updated_at, nonce, attachment) VALUES (x'21', x'22', 0, x'03', 'thumb', 'c.img', 1, 1, x'04', x'05')",
            [],
        )
        .is_err()
    );
}

#[test]
fn a_v4_upload_queue_moves_to_v5_and_takes_backdrops() {
    let conn = Connection::open_in_memory().unwrap();
    prepare(&conn).unwrap();
    for step in [V1, V2, V3, V4] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "user_version", 4).unwrap();
    let insert = "INSERT INTO media_uploads (id, album, position, conversation_id, kind, image_ref, thumb_ref, state, object, media_id, parts_sent, presign_expires_at, created_at, updated_at, nonce, attachment) VALUES (?1, x'02', ?2, x'03', ?3, 'a.img', ?4, ?5, ?6, x'09', 3, 77, 1, 2, x'04', x'05')";
    conn.execute(
        insert,
        rusqlite::params![[1u8], 0, "image", Some("a.thumb"), "uploading", "thumb"],
    )
    .unwrap();
    conn.execute(
        insert,
        rusqlite::params![[2u8], 1, "avatar", None::<&str>, "committed", "image"],
    )
    .unwrap();
    assert!(
        conn.execute(
            insert,
            rusqlite::params![[3u8], 2, "backdrop", None::<&str>, "queued", "image"],
        )
        .is_err()
    );

    migrate(&conn).unwrap();
    assert_eq!(version(&conn).unwrap(), VERSION);
    let rows: Vec<(String, String, String, Option<String>, i64, i64)> = conn
        .prepare("SELECT kind, state, object, thumb_ref, parts_sent, presign_expires_at FROM media_uploads ORDER BY position")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (
                "image".into(),
                "uploading".into(),
                "thumb".into(),
                Some("a.thumb".into()),
                3,
                77
            ),
            (
                "avatar".into(),
                "committed".into(),
                "image".into(),
                None,
                3,
                77
            ),
        ]
    );
    conn.execute(
        insert,
        rusqlite::params![[3u8], 2, "backdrop", None::<&str>, "queued", "image"],
    )
    .unwrap();
    assert!(
        conn.execute(
            insert,
            rusqlite::params![[4u8], 3, "thumb", None::<&str>, "queued", "image"],
        )
        .is_err()
    );
    let indexes: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND tbl_name = 'media_uploads' AND name IN ('media_uploads_album', 'media_uploads_open')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 2);
}

#[test]
fn a_v5_store_moves_to_v6_and_counts_the_media_of_its_posts() {
    let conn = Connection::open_in_memory().unwrap();
    prepare(&conn).unwrap();
    for step in [V1, V2, V3, V4, V5] {
        conn.execute_batch(step).unwrap();
    }
    conn.pragma_update(None, "user_version", 5).unwrap();
    for seq in [1, 2, 3] {
        conn.execute(
            "INSERT INTO messages (id, conversation_id, seq, client_id, sender, class, epoch, ts, content) VALUES (?1, x'01', ?1, x'02', 'u', 'post', 0, 0, 'open')",
            [seq],
        )
        .unwrap();
    }
    for (message_id, position) in [(1, 0), (1, 1), (3, 0)] {
        conn.execute(
            "INSERT INTO message_media (message_id, position, conversation_id, media_id, nonce, body) VALUES (?1, ?2, x'01', x'02', x'03', x'04')",
            rusqlite::params![message_id, position],
        )
        .unwrap();
    }

    migrate(&conn).unwrap();
    assert_eq!(version(&conn).unwrap(), VERSION);
    let counted: Vec<(i64, i64)> = conn
        .prepare("SELECT seq, media FROM messages ORDER BY seq")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(counted, vec![(1, 2), (2, 0), (3, 1)]);
    let indexed: Vec<i64> = conn
        .prepare("SELECT seq FROM messages INDEXED BY messages_gallery WHERE conversation_id = x'01' AND media > 0 ORDER BY seq DESC")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(indexed, vec![3, 1]);
}
