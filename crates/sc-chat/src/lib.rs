pub mod api;
pub mod channel;
pub mod contracts;
pub mod crypto;
pub mod embeds;
pub mod envelope;
#[cfg(feature = "ffi")]
pub mod ffi;
pub mod login;
pub mod outbox;
pub mod search;
pub mod store;
pub mod sync;

pub use async_trait::async_trait;
pub use contracts::*;
pub use sc_chat_wire;
pub use zeroize::Zeroizing;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();
