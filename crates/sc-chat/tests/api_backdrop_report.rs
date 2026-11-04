mod sync_support;

use sc_chat::api::wire::FileBackdropReport;
use sc_chat::{BackdropReport, ChatError, ReportReason};
use serde_json::json;
use sync_support::{Step, rig};
use uuid::Uuid;

const ROOM: Uuid = Uuid::from_u128(0x7e);
const MEDIA: Uuid = Uuid::from_u128(0x61);

fn report(comment: Option<&str>) -> FileBackdropReport {
    FileBackdropReport::from(&BackdropReport {
        conversation_id: ROOM,
        media_id: MEDIA,
        reason: ReportReason::Harassment,
        comment: comment.map(str::to_owned),
    })
}

#[tokio::test]
async fn a_backdrop_report_names_the_media_and_never_a_seq_range() {
    let rig = rig();
    rig.server.json(
        "POST",
        "/chat/reports",
        json!({"id": Uuid::from_u128(0x9), "state": "open"}),
    );

    let filed = rig
        .engine
        .api()
        .report_backdrop(&report(Some("фон")))
        .await
        .unwrap();

    assert_eq!(filed.id, Uuid::from_u128(0x9));
    let sent = &rig.server.calls_to("POST", "/chat/reports")[0].body;
    assert_eq!(
        sent,
        &json!({
            "conversation_id": ROOM,
            "kind": "backdrop",
            "media_id": MEDIA,
            "reason": "harassment",
            "comment": "фон",
        })
    );
}

#[tokio::test]
async fn a_refused_backdrop_report_keeps_its_typed_error() {
    let answers = [
        (
            429,
            json!({"error": "chat_request_limit"}),
            ChatError::RequestLimit,
        ),
        (
            409,
            json!({"error": "chat_media_missing", "ids": [MEDIA]}),
            ChatError::MediaMissing { ids: vec![MEDIA] },
        ),
        (
            403,
            json!({"error": "chat_not_member"}),
            ChatError::NotMember,
        ),
    ];
    for (status, body, error) in answers {
        let rig = rig();
        rig.server
            .on("POST", "/chat/reports", Step::Json(status, body));
        let refused = rig.engine.api().report_backdrop(&report(None)).await;
        assert_eq!(refused.err(), Some(error));
    }
}
