use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use sc_chat::KeyName;
use sc_chat_ffi::{ChatKeys, KeyFailure};

#[derive(Default)]
pub struct MemoryKeys {
    secrets: Mutex<HashMap<KeyName, Vec<u8>>>,
    denied: AtomicBool,
    calls: AtomicUsize,
}

impl MemoryKeys {
    pub fn names(&self) -> Vec<KeyName> {
        self.secrets.lock().unwrap().keys().copied().collect()
    }

    pub fn deny(&self, denied: bool) {
        self.denied.store(denied, Ordering::SeqCst);
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn ask(&self) -> Result<(), KeyFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.denied.load(Ordering::SeqCst) {
            true => Err(KeyFailure::Denied {
                reason: "the user pressed Deny".into(),
            }),
            false => Ok(()),
        }
    }
}

impl ChatKeys for MemoryKeys {
    fn load(&self, name: KeyName) -> Result<Option<Vec<u8>>, KeyFailure> {
        self.ask()?;
        Ok(self.secrets.lock().unwrap().get(&name).cloned())
    }

    fn save(&self, name: KeyName, secret: Vec<u8>) -> Result<(), KeyFailure> {
        self.ask()?;
        self.secrets.lock().unwrap().insert(name, secret);
        Ok(())
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyFailure> {
        self.ask()?;
        self.secrets.lock().unwrap().remove(&name);
        Ok(())
    }
}
