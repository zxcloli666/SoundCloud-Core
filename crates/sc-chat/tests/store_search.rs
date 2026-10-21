mod store_support;

use std::sync::Arc;

use sc_chat::search::Search;
use sc_chat::{ConvId, SearchQuery};
use store_support::{PEER, conv, edit, post, post_with_track, store};

fn search() -> (Search, ConvId) {
    let store = Arc::new(store());
    let id = conv();
    store
        .put_messages(&[
            post(id, 1, PEER, "Привет, как дела?"),
            post(id, 2, PEER, "Ёлка уже стоит"),
            post(id, 3, PEER, "İstanbul tonight"),
            post_with_track(id, 4, "слушай", "Midnight City"),
        ])
        .unwrap();
    (Search::new(store), id)
}

fn seqs(search: &Search, text: &str) -> Vec<i64> {
    let mut found: Vec<i64> = search
        .query(&SearchQuery {
            text: text.into(),
            conversation_id: None,
            limit: 20,
        })
        .unwrap()
        .into_iter()
        .map(|hit| hit.seq)
        .collect();
    found.sort();
    found
}

#[test]
fn search_finds_cyrillic_prefixes() {
    let (search, _) = search();
    assert_eq!(seqs(&search, "прив"), vec![1]);
    assert_eq!(seqs(&search, "ПРИВЕТ"), vec![1]);
    assert_eq!(seqs(&search, "как де"), vec![1]);
    assert_eq!(seqs(&search, "пока"), Vec::<i64>::new());
}

#[test]
fn yo_and_dotted_i_fold() {
    let (search, _) = search();
    assert_eq!(seqs(&search, "елк"), vec![2]);
    assert_eq!(seqs(&search, "ёлк"), vec![2]);
    assert_eq!(seqs(&search, "istan"), vec![3]);
}

#[test]
fn shared_track_titles_are_searchable() {
    let (search, _) = search();
    assert_eq!(seqs(&search, "midnight"), vec![4]);
}

#[test]
fn one_letter_queries_find_nothing() {
    let (search, _) = search();
    assert!(seqs(&search, "п").is_empty());
    assert!(seqs(&search, "  ").is_empty());
}

#[test]
fn long_words_match_by_their_first_sixteen_chars() {
    let store = Arc::new(store());
    let id = conv();
    store
        .put_messages(&[post(id, 1, PEER, "достопримечательности")])
        .unwrap();
    let search = Search::new(store);
    assert_eq!(seqs(&search, "достопримечательностями"), vec![1]);
}

#[test]
fn an_edit_replaces_the_indexed_text() {
    let (search, id) = search();
    search
        .store()
        .put_messages(&[edit(id, 5, PEER, 1, "Новый текст")])
        .unwrap();
    assert!(seqs(&search, "прив").is_empty());
    assert_eq!(seqs(&search, "новый"), vec![1]);
    let hit = &search
        .query(&SearchQuery {
            text: "текст".into(),
            conversation_id: Some(id),
            limit: 5,
        })
        .unwrap()[0];
    assert_eq!(hit.snippet, "Новый текст");
    assert_eq!(hit.sender, PEER);
}

#[test]
fn an_edit_from_someone_else_is_ignored() {
    let (search, id) = search();
    search
        .store()
        .put_messages(&[edit(id, 5, "999", 1, "Подмена")])
        .unwrap();
    assert_eq!(seqs(&search, "прив"), vec![1]);
    assert!(seqs(&search, "подмена").is_empty());
}

#[test]
fn search_can_stay_in_one_conversation() {
    let (search, id) = search();
    let other = conv();
    search
        .store()
        .put_messages(&[post(other, 1, PEER, "привет снова")])
        .unwrap();
    assert_eq!(seqs(&search, "прив").len(), 2);
    let scoped = search
        .query(&SearchQuery {
            text: "прив".into(),
            conversation_id: Some(other),
            limit: 5,
        })
        .unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].conversation_id, other);
    assert_ne!(scoped[0].conversation_id, id);
}

#[test]
fn index_and_remove_work_by_seq() {
    let (search, id) = search();
    search.remove(id, 1).unwrap();
    assert!(seqs(&search, "прив").is_empty());
    search.index(id, 1, "Заметка вручную").unwrap();
    assert_eq!(seqs(&search, "замет"), vec![1]);
    search.index(id, 99, "нет такой строки").unwrap();
    assert!(seqs(&search, "строк").is_empty());
}

#[test]
fn snippets_cut_long_text_around_the_match() {
    let store = Arc::new(store());
    let id = conv();
    let long = format!("{} находка {}", "слово ".repeat(30), "хвост ".repeat(30));
    store.put_messages(&[post(id, 1, PEER, &long)]).unwrap();
    let search = Search::new(store);
    let hit = &search
        .query(&SearchQuery {
            text: "наход".into(),
            conversation_id: None,
            limit: 5,
        })
        .unwrap()[0];
    assert!(hit.snippet.starts_with('…'));
    assert!(hit.snippet.ends_with('…'));
    assert!(hit.snippet.contains("находка"));
    assert!(hit.snippet.chars().count() <= 98);
}
