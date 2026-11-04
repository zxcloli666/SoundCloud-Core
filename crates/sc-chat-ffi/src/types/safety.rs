use sc_chat::{BackdropReport, ConvId, ReportInput, ReportReason, Seq};
use uuid::Uuid;

#[uniffi::remote(Enum)]
pub enum ReportReason {
    Spam,
    Harassment,
    Doxxing,
    Illegal,
    Other,
}

#[uniffi::remote(Record)]
pub struct ReportInput {
    pub conversation_id: ConvId,
    pub seq_from: Seq,
    pub seq_to: Seq,
    pub reason: ReportReason,
    pub comment: Option<String>,
}

#[uniffi::remote(Record)]
pub struct BackdropReport {
    pub conversation_id: ConvId,
    pub media_id: Uuid,
    pub reason: ReportReason,
    pub comment: Option<String>,
}
