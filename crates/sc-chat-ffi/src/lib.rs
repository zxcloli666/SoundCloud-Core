mod calls;
mod client;
mod client_media;
mod platform;
mod runtime;
mod setup;
mod types;

pub use client::ChatClient;
pub use platform::{ChatKeys, ChatListener, ChatTransport, KeyFailure, NetFailure};
pub use setup::ChatSetup;
pub use types::{
    MediaInfo, MediaSeal, MediaUploadRow, NewMediaUpload, OpenedMedia, PreparedMedia, PutTargets,
};

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn chat_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[uniffi::export]
pub fn post_body_len(text: String) -> u32 {
    u32::try_from(sc_chat::envelope::post_body_len(&text)).unwrap_or(u32::MAX)
}
