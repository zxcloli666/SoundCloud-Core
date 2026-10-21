use sc_chat::{ChatError, Millis};

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
