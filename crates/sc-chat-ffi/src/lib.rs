mod calls;
mod client;
mod platform;
mod runtime;
mod setup;
mod types;

pub use client::ChatClient;
pub use platform::{ChatKeys, ChatListener, ChatTransport, KeyFailure, NetFailure};
pub use setup::ChatSetup;

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn chat_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
