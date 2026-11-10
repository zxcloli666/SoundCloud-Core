use std::time::Duration;

use sc_chat::api::wire::FileBackdropReport;
use sc_chat::{BackdropReport, ChatError, ReportReason};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::backdrop::{WIDE, group, image, image_by, opens, shows};
use crate::support::{Client, Stack};

const AI_PASS: Duration = Duration::from_secs(12);

fn report_state(stack: &Stack, id: Uuid) -> String {
    stack.sql(&format!(
        "SELECT kind || ' ' || state || ' ' || coalesce(handled_by, '-') FROM chat.reports WHERE id = '{id}'"
    ))
}

fn audited(stack: &Stack, id: Uuid) -> Vec<String> {
    stack
        .sql(&format!(
            "SELECT admin || ' ' || action FROM chat.admin_audit WHERE target = 'report:{id}' ORDER BY created_at, action"
        ))
        .lines()
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
#[ignore]
async fn a_reported_group_backdrop_waits_for_the_admin_with_the_image_as_evidence() {
    let stack = Stack::from_env();
    let person = stack.person("ann");
    stack.star(&person);
    let ann = Client::up(&stack, &person).await;
    let bob = Client::up(&stack, &stack.person("bob")).await;
    stack.follow(&bob.person, &ann.person);
    let id = group(&ann, &[&bob]).await;
    let media_id = image(&ann, id, 150).await;
    let look = image_by(&ann, media_id);
    shows(&bob, id, &look).await;
    opens(&bob, id, media_id).await;
    let attachment = bob.store().backdrop_image(id).unwrap().unwrap();

    let report = FileBackdropReport::from(&BackdropReport {
        conversation_id: id,
        media_id,
        reason: ReportReason::Other,
        comment: Some("not for this group".into()),
    });
    assert_eq!(
        ann.api.report_backdrop(&report).await.err(),
        Some(ChatError::Invalid)
    );
    let filed = bob.api.report_backdrop(&report).await.unwrap();
    assert_eq!((filed.state.as_str(), filed.duplicate), ("open", false));
    let again = bob.api.report_backdrop(&report).await.unwrap();
    assert_eq!((again.id, again.duplicate), (filed.id, true));

    let path = format!("/chat-admin/reports/{}", filed.id);
    let detail = stack.admin("GET", &path, None).await;
    assert_eq!(detail.status, 200);
    let detail = detail.json();
    assert_eq!(detail["report"]["target_user"], ann.person.user_id.as_str());
    let found: Vec<_> = detail["lines"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|line| line["media"].as_array().cloned().unwrap_or_default())
        .collect();
    assert_eq!(found.len(), 1, "{detail}");
    assert_eq!(found[0]["id"], media_id.to_string());
    assert_eq!(found[0]["kind"], "backdrop");
    assert_eq!(
        (found[0]["w"].as_u64(), found[0]["h"].as_u64()),
        (Some(u64::from(WIDE.0)), Some(u64::from(WIDE.1)))
    );
    let seen = stack
        .admin("GET", &format!("{path}/media/{media_id}"), None)
        .await;
    assert_eq!(seen.status, 200);
    assert_eq!(seen.header("content-type"), Some("image/webp"));
    assert_eq!(Sha256::digest(&seen.body)[..], attachment.sha256_plain[..]);

    tokio::time::sleep(AI_PASS).await;
    assert_eq!(report_state(&stack, filed.id), "backdrop open -");
    assert_eq!(
        audited(&stack, filed.id),
        ["integration decrypt", "integration view_media"]
    );
    assert_eq!(bob.api.conversation(id).await.unwrap().backdrop, look);

    let upheld = stack
        .admin(
            "POST",
            &path,
            Some(json!({"decision": "uphold", "clear_backdrop": true})),
        )
        .await;
    assert_eq!(upheld.status, 200, "{}", upheld.json());
    assert_eq!(
        report_state(&stack, filed.id),
        "backdrop actioned integration"
    );
    for client in [&ann, &bob] {
        shows(client, id, &None).await;
    }
}
