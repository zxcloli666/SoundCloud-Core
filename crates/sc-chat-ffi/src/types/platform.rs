use sc_chat::{KeyName, Tier, TransportRequest, TransportResponse};

#[uniffi::remote(Enum)]
pub enum KeyName {
    Database,
    DeviceId,
    DeviceAuth,
    DeviceBox,
}

#[uniffi::remote(Record)]
pub struct TransportRequest {
    pub tier: Tier,
    pub envelope: Vec<u8>,
    pub timeout_ms: u64,
    pub long_poll: bool,
}

#[uniffi::remote(Record)]
pub struct TransportResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after_ms: Option<u64>,
}
