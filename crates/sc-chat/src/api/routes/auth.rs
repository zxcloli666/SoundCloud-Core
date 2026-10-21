use crate::api::path::query;
use crate::api::wire::{LinkClaim, LinkStart, LinkStatusView, LoginStatusView};
use crate::api::{Api, BindSigner, Signer};
use crate::contracts::{
    ChatResult, LinkClaimed, LinkCreated, LinkMode, LinkPoll, LoginBegin, LoginPoll,
};

impl Api {
    pub async fn login_begin(&self, signer: &BindSigner) -> ChatResult<LoginBegin> {
        self.json_as(&bind(signer), "GET", "/auth/login", "", None::<&()>)
            .await
    }

    pub async fn login_status(&self, signer: &BindSigner, id: &str) -> ChatResult<LoginPoll> {
        let query = query(&[("id", Some(id.to_owned()))]);
        let view: LoginStatusView = self
            .json_as(
                &bind(signer),
                "GET",
                "/auth/login/status",
                &query,
                None::<&()>,
            )
            .await?;
        Ok(view.into())
    }

    pub async fn link_create_pull(&self, signer: &BindSigner) -> ChatResult<LinkCreated> {
        let body = LinkStart {
            mode: LinkMode::Pull,
        };
        self.json_as(&bind(signer), "POST", "/auth/link/create", "", Some(&body))
            .await
    }

    pub async fn link_claim_push(
        &self,
        signer: &BindSigner,
        claim_token: &str,
    ) -> ChatResult<LinkClaimed> {
        let body = LinkClaim {
            claim_token: claim_token.to_owned(),
        };
        self.json_as(&bind(signer), "POST", "/auth/link/claim", "", Some(&body))
            .await
    }

    pub async fn link_status(&self, signer: &BindSigner, id: &str) -> ChatResult<LinkPoll> {
        let query = query(&[("id", Some(id.to_owned()))]);
        let view: LinkStatusView = self
            .json_as(
                &bind(signer),
                "GET",
                "/auth/link/status",
                &query,
                None::<&()>,
            )
            .await?;
        Ok(view.into())
    }
}

fn bind(signer: &BindSigner) -> Signer {
    Signer::Bind(signer.clone())
}
