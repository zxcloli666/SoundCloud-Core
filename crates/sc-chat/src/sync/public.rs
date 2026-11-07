use super::SyncEngine;
use super::local::Local;
use crate::api::wire::{MuteView, NewcomerView};
use crate::contracts::{ChatResult, ModeratorMute, Newcomer, PublicOpened};

impl<L: Local> SyncEngine<L> {
    pub async fn open_public(&self) -> ChatResult<PublicOpened> {
        let opened = self.api.open_public().await?;
        let conversation = self.adopt(&opened.conversation).await?;
        Ok(PublicOpened {
            conversation,
            online: opened.online,
            mute: opened.mute.map(mute),
            newcomer: opened.newcomer.map(newcomer),
        })
    }
}

fn mute(view: MuteView) -> ModeratorMute {
    ModeratorMute {
        until: view.until,
        reason: view.reason,
    }
}

fn newcomer(view: NewcomerView) -> Newcomer {
    Newcomer {
        until: view.until,
        body_max: view.body_max,
    }
}
