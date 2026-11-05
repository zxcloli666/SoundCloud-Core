use std::path::PathBuf;

use super::put_back;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_rollback_stops_at_the_first_file_it_cannot_put_back() {
    let dir =
        Scratch(std::env::temp_dir().join(format!("sc-chat-put-back-{}", uuid::Uuid::new_v4())));
    let live = dir.0.join("chat.db");
    let live_wal = dir.0.join("chat.db-wal");
    let orphan = dir.0.join("chat.db.orphan-9");
    let orphan_wal = dir.0.join("chat.db.orphan-9-wal");
    std::fs::create_dir_all(live_wal.join("taken")).unwrap();
    std::fs::write(&orphan, b"db").unwrap();
    std::fs::write(&orphan_wal, b"wal").unwrap();

    put_back(&[
        (live.clone(), orphan.clone()),
        (live_wal, orphan_wal.clone()),
    ]);

    assert!(!live.exists());
    assert_eq!(std::fs::read(&orphan).unwrap(), b"db");
    assert_eq!(std::fs::read(&orphan_wal).unwrap(), b"wal");
}
