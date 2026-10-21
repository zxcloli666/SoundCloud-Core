use std::sync::{Arc, Mutex};

use sc_chat::{ChatStatus, CoreEvent, Events};

use super::lock;
use crate::platform::ChatListener;

pub struct Feed {
    listener: Arc<dyn ChatListener>,
    status: Mutex<ChatStatus>,
}

impl Feed {
    pub fn new(listener: Arc<dyn ChatListener>) -> Self {
        Self {
            listener,
            status: Mutex::new(ChatStatus::off()),
        }
    }

    pub fn status(&self) -> ChatStatus {
        lock(&self.status).clone()
    }

    pub fn update(&self, change: impl FnOnce(&mut ChatStatus)) {
        let changed = {
            let mut status = lock(&self.status);
            let before = status.clone();
            change(&mut status);
            (*status != before).then(|| status.clone())
        };
        if let Some(status) = changed {
            self.listener.on_event(CoreEvent::Status(status));
        }
    }
}

impl Events for Feed {
    fn emit(&self, event: CoreEvent) {
        match event {
            CoreEvent::Status(engine) => self.update(|status| take_engine_part(status, &engine)),
            event => self.listener.on_event(event),
        }
    }
}

fn take_engine_part(status: &mut ChatStatus, engine: &ChatStatus) {
    status.online = engine.online;
    status.tier = engine.tier;
    status.history = engine.history.clone();
    status.unread = engine.unread;
    status.mentions = engine.mentions;
    status.requests = engine.requests;
}
