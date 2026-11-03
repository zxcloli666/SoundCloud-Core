use sc_chat::{ChatError, Millis};
use uuid::Uuid;

#[uniffi::remote(Error)]
pub enum ChatError {
    NotMember,
    EpochStale,
    Muted {
        until: Millis,
    },
    SlowMode {
        retry_ms: u64,
    },
    Unavailable,
    RequestLimit,
    SendLimited,
    ReadLimited,
    Busy {
        retry_ms: u64,
    },
    Clock {
        server_now: Millis,
    },
    Replay,
    DeviceRevoked,
    ReloginRequired,
    SessionBoundOther,
    TooYoung,
    Disabled,
    SendPaused,
    QuotaExceeded {
        used: u64,
        quota: u64,
    },
    MediaStarRequired,
    RoomStarRequired,
    MediaUploadsPaused,
    MediaMissing {
        ids: Vec<Uuid>,
    },
    Invalid,
    NotFound,
    Unimplemented,
    Internal,
    Offline,
    NotReady,
    Store,
    Keychain,
    Crypto,
    #[uniffi(name = "ProtocolError")]
    Protocol,
}
