use serde::{Deserialize, Serialize};

use crate::contracts::{LinkMode, LinkPoll, LinkState, LoginPoll, LoginState, LoginStep};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkStart {
    pub mode: LinkMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkClaim {
    pub claim_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStatusView {
    pub status: String,
    pub step: Option<String>,
    pub session_id: Option<String>,
    pub username: Option<String>,
    pub error: Option<String>,
    pub redirect_url: Option<String>,
}

impl From<LoginStatusView> for LoginPoll {
    fn from(view: LoginStatusView) -> Self {
        LoginPoll {
            status: match view.status.as_str() {
                "pending" | "processing" => LoginState::Pending,
                "completed" => LoginState::Completed,
                "expired" => LoginState::Expired,
                _ => LoginState::Failed,
            },
            step: view.step.as_deref().and_then(|step| match step {
                "token" => Some(LoginStep::Token),
                "extract" => Some(LoginStep::Extract),
                "finalizing" => Some(LoginStep::Finalizing),
                _ => None,
            }),
            session_id: view.session_id,
            username: view.username,
            error: view.error,
            redirect_url: view.redirect_url,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatusView {
    pub status: String,
    pub mode: LinkMode,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

impl From<LinkStatusView> for LinkPoll {
    fn from(view: LinkStatusView) -> Self {
        LinkPoll {
            status: match view.status.as_str() {
                "pending" => LinkState::Pending,
                "claimed" | "completed" => LinkState::Claimed,
                "expired" => LinkState::Expired,
                _ => LinkState::Failed,
            },
            mode: view.mode,
            session_id: view.session_id,
            error: view.error,
        }
    }
}
