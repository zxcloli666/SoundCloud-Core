#[uniffi::export]
pub fn sc_chat_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
