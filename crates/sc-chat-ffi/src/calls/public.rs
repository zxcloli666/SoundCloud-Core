use sc_chat::{ChatResult, PublicOpened};

use crate::runtime::Chat;

impl Chat {
    pub async fn open_public(&self) -> ChatResult<PublicOpened> {
        let core = self.ready()?;
        let opened = core.sync.open_public().await?;
        core.sync.kick();
        Ok(opened)
    }
}
