use sc_chat::{
    LinkClaimed, LinkCreated, LinkMode, LinkPoll, LinkState, LoginBegin, LoginPoll, LoginState,
    LoginStep, LoginTarget, Millis,
};

#[uniffi::remote(Enum)]
pub enum LoginTarget {
    Fresh,
    Current,
}

#[uniffi::remote(Record)]
pub struct LoginBegin {
    pub url: String,
    pub login_request_id: String,
}

#[uniffi::remote(Enum)]
pub enum LoginState {
    Pending,
    Completed,
    Failed,
    Expired,
}

#[uniffi::remote(Enum)]
pub enum LoginStep {
    Token,
    Extract,
    Finalizing,
}

#[uniffi::remote(Record)]
pub struct LoginPoll {
    pub status: LoginState,
    pub step: Option<LoginStep>,
    pub session_id: Option<String>,
    pub username: Option<String>,
    pub error: Option<String>,
    pub redirect_url: Option<String>,
}

#[uniffi::remote(Enum)]
pub enum LinkMode {
    Pull,
    Push,
}

#[uniffi::remote(Enum)]
pub enum LinkState {
    Pending,
    Claimed,
    Failed,
    Expired,
}

#[uniffi::remote(Record)]
pub struct LinkCreated {
    pub link_request_id: String,
    pub claim_token: String,
    pub expires_at: Millis,
    pub mode: LinkMode,
}

#[uniffi::remote(Record)]
pub struct LinkPoll {
    pub status: LinkState,
    pub mode: LinkMode,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[uniffi::remote(Record)]
pub struct LinkClaimed {
    pub session_id: String,
    pub mode: LinkMode,
}
