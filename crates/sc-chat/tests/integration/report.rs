use sc_chat::api::wire::FileReport;
use serde_json::Value;

use crate::dm::direct;
use crate::support::{Client, Stack};

async fn with_evidence(stack: &Stack, path: &str) -> Value {
    for _ in 0..40 {
        let reply = stack.admin("GET", path, None).await;
        let detail = reply.json();
        if reply.status == 200 && detail["report"]["has_evidence"] == Value::Bool(true) {
            return detail;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("the report at {path} got no evidence within 10 s");
}

#[tokio::test]
#[ignore]
async fn a_report_keeps_evidence_that_the_admin_decrypts_with_an_audit_row() {
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
    assert_eq!(ann.send(view.id, "context before").await, Ok(1));
    assert_eq!(ann.send(view.id, "the reported line").await, Ok(1));
    bob.sync_until("bob gets both", |bob| bob.texts(view.id).len() == 2)
        .await;
    let reported = bob.messages(view.id)[1].seq.unwrap();

    let filed = bob
        .api
        .report(&FileReport {
            conversation_id: view.id,
            seq_from: reported,
            seq_to: reported,
            reason: "harassment".into(),
            comment: Some("not nice".into()),
        })
        .await
        .unwrap();

    let path = format!("/chat-admin/reports/{}", filed.id);
    let detail = with_evidence(&stack, &path).await;
    assert_eq!(detail["report"]["target_user"], ann.person.user_id.as_str());
    let lines = detail["lines"].as_array().unwrap();
    let line = lines
        .iter()
        .find(|line| line["seq"] == reported)
        .expect("the reported seq is in the evidence");
    assert_eq!(line["text"], "the reported line");
    assert_eq!(line["reported"], true);
    assert!(
        lines
            .iter()
            .any(|line| line["text"] == "context before" && line["reported"] == false)
    );
    let audited = stack.admin("GET", &path, None).await.json();
    assert!(audited["audit"].as_array().unwrap().iter().any(|row| {
        row["admin"] == "integration"
            && row["action"]
                .as_str()
                .unwrap_or_default()
                .contains("decrypt")
    }));
    let log = stack.log();
    for text in ["context before", "the reported line"] {
        assert!(!log.contains(text), "decrypted evidence reached the log");
    }
}
