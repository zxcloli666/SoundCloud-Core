use std::time::Duration;

use sc_chat::api::wire::FileReport;
use sc_chat::{ChatError, ConvId};
use serde_json::Value;
use uuid::Uuid;

use crate::dm::direct;
use crate::support::{Client, Stack, within};

const JUDGED_WITHIN: Duration = Duration::from_secs(40);
const MODERATOR: &str = "ai-moderator";
const HARMLESS: &str = "loved your last mix, the drop at two minutes is great";
const HARASSMENT: &str = "you are worthless and nobody wants you here";
const REPEAT: &str = "you are worthless, nobody wants you, just leave";

async fn report(reporter: &Client, conversation_id: ConvId, seq: i64, reason: &str) -> Uuid {
    reporter
        .api
        .report(&FileReport {
            conversation_id,
            seq_from: seq,
            seq_to: seq,
            reason: reason.into(),
            comment: None,
        })
        .await
        .expect("the report is filed")
        .id
}

async fn judged(stack: &Stack, id: Uuid) -> String {
    let mut state = String::new();
    within("the ai moderator closes the report", JUDGED_WITHIN, || {
        let found = stack.sql(&format!(
            "SELECT state || ' ' || coalesce(handled_by, '-') FROM chat.reports WHERE id = '{id}'"
        ));
        let done = !found.starts_with("open");
        if done {
            state = found;
        }
        async move { done }
    })
    .await;
    state
}

fn audit(stack: &Stack, target: &str) -> Vec<String> {
    stack
        .sql(&format!(
            "SELECT action FROM chat.admin_audit WHERE admin = '{MODERATOR}' AND target = '{target}' ORDER BY created_at, action"
        ))
        .lines()
        .map(str::to_owned)
        .collect()
}

fn verdict(stack: &Stack, id: Uuid) -> Value {
    let raw = stack.sql(&format!(
        "SELECT detail FROM chat.admin_audit WHERE admin = '{MODERATOR}' AND target = 'report:{id}' AND action = 'verdict'"
    ));
    serde_json::from_str(&raw).expect("the verdict audit row is json")
}

fn body_kept(stack: &Stack, conversation_id: ConvId, seq: i64) -> bool {
    stack.sql(&format!(
        "SELECT body IS NOT NULL FROM chat.messages WHERE conversation_id = '{conversation_id}' AND seq = {seq}"
    )) == "t"
}

fn seq_of(client: &Client, conversation_id: ConvId, text: &str) -> i64 {
    client
        .message(conversation_id, text)
        .and_then(|message| message.seq)
        .expect("the message is on the device")
}

#[tokio::test]
#[ignore]
async fn the_ai_moderator_rejects_a_harmless_report_and_removes_harassment_then_bans_a_repeat() {
    let stack = Stack::from_env();
    let ann = Client::up(&stack, &stack.person("ann")).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    stack.follow(&bob.person, &ann.person);
    let view = ann
        .api
        .create_conversation(&direct(&bob.person.user_id))
        .await
        .unwrap();
    ann.engine.adopt(&view).await.unwrap();
    assert_eq!(ann.send(view.id, HARMLESS).await, Ok(1));
    assert_eq!(ann.send(view.id, HARASSMENT).await, Ok(1));
    bob.sync_until("bob gets both lines", |bob| {
        bob.texts(view.id) == [HARMLESS, HARASSMENT]
    })
    .await;
    let harmless_seq = seq_of(&bob, view.id, HARMLESS);
    let harassment_seq = seq_of(&bob, view.id, HARASSMENT);

    let harmless = report(&bob, view.id, harmless_seq, "spam").await;
    let harassment = report(&bob, view.id, harassment_seq, "harassment").await;
    assert_eq!(
        judged(&stack, harmless).await,
        format!("rejected {MODERATOR}")
    );
    assert_eq!(
        judged(&stack, harassment).await,
        format!("actioned {MODERATOR}")
    );

    assert_eq!(
        audit(&stack, &format!("report:{harmless}")),
        ["decrypt", "prompt", "verdict", "reject"]
    );
    assert_eq!(
        audit(&stack, &format!("report:{harassment}")),
        ["decrypt", "prompt", "verdict", "delete_messages", "uphold"]
    );
    for id in [harmless, harassment] {
        let detail = verdict(&stack, id);
        assert_eq!(detail["outcome"], "answered");
        assert!(
            detail["model"]
                .as_str()
                .is_some_and(|model| !model.is_empty())
        );
        assert_eq!(detail["prompt_sha256"].as_str().map(str::len), Some(64));
    }
    assert_eq!(
        verdict(&stack, harassment)["verdict"]["category"],
        "harassment"
    );
    assert!(body_kept(&stack, view.id, harmless_seq));
    assert!(!body_kept(&stack, view.id, harassment_seq));
    assert!(audit(&stack, &format!("user:{}", ann.person.user_id)).is_empty());

    for client in [&ann, &bob] {
        client
            .sync_until("the removed line leaves every device", |client| {
                client.texts(view.id) == [HARMLESS]
            })
            .await;
        assert!(
            client
                .events
                .gone_messages()
                .iter()
                .any(|gone| { gone.conversation_id == view.id && gone.seq == harassment_seq })
        );
    }

    assert_eq!(ann.send(view.id, REPEAT).await, Ok(1));
    bob.sync_until("bob gets the repeat", |bob| {
        bob.texts(view.id) == [HARMLESS, REPEAT]
    })
    .await;
    let repeat = report(&bob, view.id, seq_of(&bob, view.id, REPEAT), "harassment").await;
    assert_eq!(
        judged(&stack, repeat).await,
        format!("actioned {MODERATOR}")
    );
    assert_eq!(
        audit(&stack, &format!("user:{}", ann.person.user_id)),
        ["ban"]
    );
    let banned_for_hours = stack.count(&format!(
        "SELECT round(extract(epoch FROM restricted_until - now()) / 3600) FROM chat.users WHERE user_id = '{}'",
        ann.person.user_id
    ));
    assert_eq!(banned_for_hours, 7 * 24);
    assert!(matches!(
        ann.raw_post(view.id).await,
        Err(ChatError::Muted { .. })
    ));

    let log = stack.log();
    assert!(
        !log.contains(stack.ai_key()),
        "the model key reached the log"
    );
    for text in [HARMLESS, HARASSMENT, REPEAT] {
        assert!(!log.contains(text), "reported text reached the log");
    }
}
