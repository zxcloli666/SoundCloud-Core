use sc_chat::{ConvId, ReportInput, ReportReason, Seq};

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
