use super::IMAGE_POSTS;
use crate::store::Store;

#[test]
fn the_gallery_walks_its_own_index_newest_first_and_never_sorts() {
    let store = Store::open_in_memory(&[3; 32]).unwrap();
    let conn = store.conn().unwrap();
    let plan: Vec<String> = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN SELECT id FROM messages WHERE {IMAGE_POSTS}"
        ))
        .unwrap()
        .query_map(rusqlite::params![[0u8; 16], i64::MAX, 10], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("messages_gallery")),
        "{plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "{plan:?}"
    );
    assert!(
        !plan.iter().any(|step| step.starts_with("SCAN")),
        "{plan:?}"
    );
}
