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
    DeviceCap,
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
    BackdropCooldown {
        until: Millis,
    },
    MediaUploadsPaused,
    MediaMissing {
        ids: Vec<Uuid>,
    },
    Invalid,
    NotFound,
    Internal,
    Offline,
    NotReady,
    Store,
    Keychain,
    KeychainDenied,
    Crypto,
    #[uniffi(name = "ProtocolError")]
    Protocol,
}
