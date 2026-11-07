use sc_chat::{ConversationSummary, Millis, ModeratorMute, Newcomer, PublicOpened};

#[uniffi::remote(Record)]
pub struct PublicOpened {
    pub conversation: ConversationSummary,
    pub online: u32,
    pub mute: Option<ModeratorMute>,
    pub newcomer: Option<Newcomer>,
}

#[uniffi::remote(Record)]
pub struct ModeratorMute {
    pub until: Millis,
    pub reason: Option<String>,
}

#[uniffi::remote(Record)]
pub struct Newcomer {
    pub until: Option<Millis>,
    pub body_max: u32,
}
