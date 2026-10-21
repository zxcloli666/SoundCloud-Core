use std::sync::Mutex;

use sc_chat::{
    ChatStatus, CoreEvent, DeviceAddedEvent, Events, GoneEvent, MessagesEvent, SyncEvent,
    TypingEvent,
};

#[derive(Default)]
pub struct Recorder {
    seen: Mutex<Vec<CoreEvent>>,
}

impl Events for Recorder {
    fn emit(&self, event: CoreEvent) {
        self.seen.lock().unwrap().push(event);
    }
}

impl Recorder {
    pub fn all(&self) -> Vec<CoreEvent> {
        self.seen.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.seen.lock().unwrap().clear();
    }

    pub fn gone(&self) -> Vec<GoneEvent> {
        self.pick(|event| match event {
            CoreEvent::Gone(gone) => Some(gone),
            _ => None,
        })
    }

    pub fn messages(&self) -> Vec<MessagesEvent> {
        self.pick(|event| match event {
            CoreEvent::Messages(messages) => Some(messages),
            _ => None,
        })
    }

    pub fn syncs(&self) -> Vec<SyncEvent> {
        self.pick(|event| match event {
            CoreEvent::Sync(sync) => Some(sync),
            _ => None,
        })
    }

    pub fn typing(&self) -> Vec<TypingEvent> {
        self.pick(|event| match event {
            CoreEvent::Typing(typing) => Some(typing),
            _ => None,
        })
    }

    pub fn devices(&self) -> Vec<DeviceAddedEvent> {
        self.pick(|event| match event {
            CoreEvent::DeviceAdded(added) => Some(added),
            _ => None,
        })
    }

    pub fn statuses(&self) -> Vec<ChatStatus> {
        self.pick(|event| match event {
            CoreEvent::Status(status) => Some(status),
            _ => None,
        })
    }

    fn pick<T>(&self, choose: impl Fn(CoreEvent) -> Option<T>) -> Vec<T> {
        self.all().into_iter().filter_map(choose).collect()
    }
}
