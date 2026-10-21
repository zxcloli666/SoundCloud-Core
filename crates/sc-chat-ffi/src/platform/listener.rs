use sc_chat::CoreEvent;

#[uniffi::export(with_foreign)]
pub trait ChatListener: Send + Sync {
    fn on_event(&self, event: CoreEvent);
}
