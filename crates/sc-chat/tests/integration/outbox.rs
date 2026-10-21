use std::path::PathBuf;

use sc_chat::{ConvId, Tier};
use uuid::Uuid;

use crate::dm::direct;
use crate::support::{Client, Person, Stack, eventually};

const DEAD: &str = "http://127.0.0.1:9";

struct Disk {
    path: PathBuf,
    master: [u8; 32],
}

impl Disk {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("chat-it-{}.db", Uuid::new_v4())),
            master: sc_chat::sc_chat_wire::random_bytes().unwrap(),
        }
    }

    fn client(&self, origin: &str, person: &Person) -> Client {
        Client::on_disk(
            vec![(Tier::Direct, origin.to_owned())],
            person,
            &self.path,
            &self.master,
        )
    }
}

impl Drop for Disk {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.path.display()));
        }
    }
}

async fn flushed_after_restart(client: &Client) {
    eventually(
        "the restarted client sends what its outbox kept",
        || async { client.engine.flush_outbox().await.unwrap_or_default() == 1 },
    )
    .await;
}

fn count_of(client: &Client, conversation_id: ConvId, text: &str) -> usize {
    client
        .texts(conversation_id)
        .iter()
        .filter(|seen| *seen == text)
        .count()
}

#[tokio::test]
#[ignore]
async fn an_outbox_written_offline_or_with_a_lost_answer_is_sent_once_after_a_restart() {
    let stack = Stack::from_env();
    let bob = Client::up(&stack, &stack.person("bob")).await;
    let person = stack.person("ann");
    stack.follow(&bob.person, &person);
    let disk = Disk::new();

    let first = disk.client(&stack.origin, &person);
    first.api.config().await.unwrap();
    first.register().await.unwrap();
    first.sync().await.unwrap();
    let view = first
        .api
        .create_conversation(&direct(&bob.person.user_id))
        .await
        .unwrap();
    first.engine.adopt(&view).await.unwrap();
    assert_eq!(first.send(view.id, "online").await, Ok(1));
    drop(first);

    let offline = disk.client(DEAD, &person);
    offline.queue(view.id, "written offline").unwrap();
    assert_eq!(offline.engine.flush_outbox().await, Ok(0));
    drop(offline);

    let back = disk.client(&stack.origin, &person);
    flushed_after_restart(&back).await;
    assert_eq!(count_of(&back, view.id, "written offline"), 1);

    back.transport.lose_answers(true);
    back.queue(view.id, "answer lost").unwrap();
    assert_eq!(back.engine.flush_outbox().await, Ok(0));
    drop(back);

    let again = disk.client(&stack.origin, &person);
    flushed_after_restart(&again).await;
    again
        .sync_until("the restarted client settles its own copy", |ann| {
            count_of(ann, view.id, "answer lost") == 1
        })
        .await;
    assert_eq!(
        again.texts(view.id),
        ["online", "written offline", "answer lost"]
    );
    bob.sync_until("bob gets every message exactly once", |bob| {
        bob.texts(view.id) == ["online", "written offline", "answer lost"]
    })
    .await;
    assert_eq!(
        stack.count(&format!(
            "SELECT count(*) FROM chat.messages WHERE conversation_id = '{}' AND class = 'post'",
            view.id
        )),
        3
    );
}
