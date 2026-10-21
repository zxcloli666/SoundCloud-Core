use std::sync::Arc;
use std::time::{Duration, Instant};

use sc_chat::api::wire::{LinkClaim, LinkStart};
use sc_chat::api::{BindSigner, Signer};
use sc_chat::crypto::DeviceKeys;
use sc_chat::{
    ChatError, ChatResult, LinkClaimed, LinkCreated, LinkMode, LinkPoll, LinkState, LoginBegin,
    LoginPoll, LoginState, LoginTarget,
};

use crate::runtime::{Chat, Held};

const LINK_CREATE: &str = "/auth/link/create";
const LINK_CLAIM: &str = "/auth/link/claim";
const HOLD_FOR: Duration = Duration::from_secs(20 * 60);

impl Chat {
    pub async fn login_begin(&self, target: LoginTarget) -> ChatResult<LoginBegin> {
        let session = match target {
            LoginTarget::Fresh => None,
            LoginTarget::Current => Some(self.session_id()?),
        };
        let signer = self.signer(session).await?;
        let begun = self.api.login_begin(&signer).await?;
        self.hold(&begun.login_request_id, &signer);
        Ok(begun)
    }

    pub async fn login_poll(&self, login_request_id: &str) -> ChatResult<LoginPoll> {
        let signer = self.held_signer(login_request_id).await?;
        let polled = self.api.login_status(&signer, login_request_id).await?;
        if polled.status != LoginState::Pending {
            self.held().remove(login_request_id);
        }
        Ok(polled)
    }

    pub async fn link_create(&self, mode: LinkMode) -> ChatResult<LinkCreated> {
        let session = match mode {
            LinkMode::Pull => None,
            LinkMode::Push => Some(self.session_id()?),
        };
        let signer = self.signer(session).await?;
        let body = LinkStart { mode };
        let created: LinkCreated = self
            .api
            .json_as(
                &Signer::Bind(signer.clone()),
                "POST",
                LINK_CREATE,
                "",
                Some(&body),
            )
            .await?;
        self.hold(&created.link_request_id, &signer);
        Ok(created)
    }

    pub async fn link_claim(&self, mode: LinkMode, claim_token: &str) -> ChatResult<LinkClaimed> {
        let session = match mode {
            LinkMode::Pull => Some(self.session_id()?),
            LinkMode::Push => None,
        };
        let signer = self.signer(session).await?;
        let body = LinkClaim {
            claim_token: claim_token.to_owned(),
        };
        self.api
            .json_as(&Signer::Bind(signer), "POST", LINK_CLAIM, "", Some(&body))
            .await
    }

    pub async fn link_poll(&self, link_request_id: &str) -> ChatResult<LinkPoll> {
        let signer = self.held_signer(link_request_id).await?;
        let polled = self.api.link_status(&signer, link_request_id).await?;
        if polled.status != LinkState::Pending {
            self.held().remove(link_request_id);
        }
        Ok(polled)
    }

    async fn signer(&self, target_session: Option<String>) -> ChatResult<BindSigner> {
        let keys = self.keys.clone();
        let (device_id, device) = self
            .handle
            .spawn_blocking(move || DeviceKeys::load_or_create(keys.as_ref()))
            .await
            .map_err(|_| ChatError::Keychain)??;
        Ok(BindSigner {
            device_id,
            keys: Arc::new(device),
            target_session,
        })
    }

    async fn held_signer(&self, request_id: &str) -> ChatResult<BindSigner> {
        let held = self.held().get(request_id).map(|held| BindSigner {
            device_id: held.device_id,
            keys: held.keys.clone(),
            target_session: None,
        });
        match held {
            Some(signer) => Ok(signer),
            None => self.signer(None).await,
        }
    }

    fn hold(&self, request_id: &str, signer: &BindSigner) {
        let mut held = self.held();
        held.retain(|_, entry| entry.since.elapsed() < HOLD_FOR);
        held.insert(
            request_id.to_owned(),
            Held {
                device_id: signer.device_id,
                keys: signer.keys.clone(),
                since: Instant::now(),
            },
        );
    }
}
