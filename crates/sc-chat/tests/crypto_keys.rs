use std::collections::HashMap;
use std::sync::Mutex;

use sc_chat::crypto::{CkCache, DeviceKeys};
use sc_chat::sc_chat_wire::ck::seal_for_device;
use sc_chat::sc_chat_wire::random_bytes;
use sc_chat::{ChatError, ChatResult, KeyName, KeyStorage, KeyStore, Zeroizing};
use uuid::Uuid;

#[derive(Default)]
struct MemoryKeys {
    saved: Mutex<HashMap<KeyName, Vec<u8>>>,
    broken: bool,
}

impl MemoryKeys {
    fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    fn put(&self, name: KeyName, secret: &[u8]) {
        self.saved.lock().unwrap().insert(name, secret.to_vec());
    }

    fn has(&self, name: KeyName) -> bool {
        self.saved.lock().unwrap().contains_key(&name)
    }
}

impl KeyStore for MemoryKeys {
    fn storage(&self) -> KeyStorage {
        KeyStorage::Os
    }

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>> {
        if self.broken {
            return Err(ChatError::Keychain);
        }
        Ok(self
            .saved
            .lock()
            .unwrap()
            .get(&name)
            .cloned()
            .map(Zeroizing::new))
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        self.put(name, secret);
        Ok(())
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.saved.lock().unwrap().remove(&name);
        Ok(())
    }
}

#[test]
fn device_keys_are_created_once_and_then_loaded() {
    let store = MemoryKeys::default();
    let (device_id, keys) = DeviceKeys::load_or_create(&store).unwrap();
    assert_eq!(device_id.get_version_num(), 4);
    for name in [KeyName::DeviceId, KeyName::DeviceAuth, KeyName::DeviceBox] {
        assert!(store.has(name), "{}", name.as_str());
    }
    assert!(!store.has(KeyName::Database));
    let (again, loaded) = DeviceKeys::load_or_create(&store).unwrap();
    assert_eq!(again, device_id);
    assert_eq!(loaded.auth_public(), keys.auth_public());
    assert_eq!(loaded.box_public().unwrap(), keys.box_public().unwrap());
}

struct SlowKeys(MemoryKeys);

impl KeyStore for SlowKeys {
    fn storage(&self) -> KeyStorage {
        KeyStorage::Os
    }

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>> {
        self.0.load(name)
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        std::thread::sleep(std::time::Duration::from_millis(20));
        self.0.save(name, secret)
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.0.delete(name)
    }
}

#[test]
fn two_first_starts_at_once_end_with_one_identity() {
    let store = std::sync::Arc::new(SlowKeys(MemoryKeys::default()));
    let starts: Vec<_> = (0..4)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                let (device_id, keys) = DeviceKeys::load_or_create(store.as_ref()).unwrap();
                (device_id, keys.auth_public())
            })
        })
        .collect();
    let made: Vec<_> = starts
        .into_iter()
        .map(|start| start.join().unwrap())
        .collect();
    let (stored_id, stored_keys) = DeviceKeys::load_or_create(store.as_ref()).unwrap();
    assert!(
        made.iter()
            .all(|(id, auth)| *id == stored_id && *auth == stored_keys.auth_public())
    );
}

#[test]
fn a_half_written_identity_is_replaced_whole() {
    let store = MemoryKeys::default();
    let (device_id, keys) = DeviceKeys::load_or_create(&store).unwrap();
    store.put(KeyName::DeviceAuth, &[1, 2, 3]);
    let (fresh, replaced) = DeviceKeys::load_or_create(&store).unwrap();
    assert_ne!(fresh, device_id);
    assert_ne!(replaced.auth_public(), keys.auth_public());
    let (loaded, _) = DeviceKeys::load_or_create(&store).unwrap();
    assert_eq!(loaded, fresh);
}

#[test]
fn a_keychain_that_fails_never_rotates_the_device() {
    let store = MemoryKeys::broken();
    assert_eq!(
        DeviceKeys::load_or_create(&store).err(),
        Some(ChatError::Keychain)
    );
    assert!(!store.has(KeyName::DeviceId));
}

#[test]
fn erase_forgets_the_device() {
    let store = MemoryKeys::default();
    let (device_id, _) = DeviceKeys::load_or_create(&store).unwrap();
    DeviceKeys::erase(&store).unwrap();
    for name in [KeyName::DeviceId, KeyName::DeviceAuth, KeyName::DeviceBox] {
        assert!(!store.has(name));
    }
    let (next, _) = DeviceKeys::load_or_create(&store).unwrap();
    assert_ne!(next, device_id);
}

fn sealed_ck(keys: &DeviceKeys, conv: Uuid, epoch: u32, device: Uuid, ck: &[u8; 32]) -> Vec<u8> {
    let box_pub = keys.box_public().unwrap();
    let ikm_e = random_bytes().unwrap();
    seal_for_device(
        &box_pub,
        conv.as_bytes(),
        epoch,
        device.as_bytes(),
        ck,
        ikm_e,
    )
    .unwrap()
}

#[test]
fn a_delivered_ck_opens_once_and_stays_cached() {
    let keys = DeviceKeys::generate().unwrap();
    let device = Uuid::new_v4();
    let conv = Uuid::new_v4();
    let ck = random_bytes::<32>().unwrap();
    let sealed = sealed_ck(&keys, conv, 4, device, &ck);
    let cache = CkCache::default();
    assert!(cache.get(conv, 4).is_none());
    let opened = cache.open(&keys, conv, 4, device, &sealed).unwrap();
    assert_eq!(opened.as_bytes(), &ck);
    assert_eq!(cache.get(conv, 4).unwrap().as_bytes(), &ck);
    assert_eq!(
        cache.open(&keys, conv, 5, device, &sealed).err(),
        Some(ChatError::Crypto)
    );
    assert!(cache.get(conv, 5).is_none());
}

#[test]
fn a_ck_sealed_to_another_device_does_not_open() {
    let keys = DeviceKeys::generate().unwrap();
    let other = DeviceKeys::generate().unwrap();
    let device = Uuid::new_v4();
    let conv = Uuid::new_v4();
    let sealed = sealed_ck(&other, conv, 1, device, &[5; 32]);
    assert_eq!(
        keys.open_ck(conv, 1, device, &sealed).err(),
        Some(ChatError::Crypto)
    );
    let mine = sealed_ck(&keys, conv, 1, device, &[5; 32]);
    assert_eq!(
        keys.open_ck(conv, 1, Uuid::new_v4(), &mine).err(),
        Some(ChatError::Crypto)
    );
}

#[test]
fn the_ck_cache_forgets_a_conversation_and_drops_the_oldest_keys() {
    let keys = DeviceKeys::generate().unwrap();
    let device = Uuid::new_v4();
    let cache = CkCache::new(2);
    let gone = Uuid::new_v4();
    let kept = Uuid::new_v4();
    for (conv, epoch) in [(gone, 1), (kept, 1)] {
        let sealed = sealed_ck(&keys, conv, epoch, device, &[epoch as u8; 32]);
        cache.open(&keys, conv, epoch, device, &sealed).unwrap();
    }
    cache.forget(gone);
    assert!(cache.get(gone, 1).is_none());
    assert_eq!(cache.len(), 1);
    let sealed = sealed_ck(&keys, kept, 2, device, &[2; 32]);
    cache.open(&keys, kept, 2, device, &sealed).unwrap();
    let sealed = sealed_ck(&keys, kept, 3, device, &[3; 32]);
    cache.open(&keys, kept, 3, device, &sealed).unwrap();
    assert!(cache.get(kept, 1).is_none());
    assert_eq!(cache.get(kept, 3).unwrap().as_bytes(), &[3; 32]);
    cache.clear();
    assert!(cache.is_empty());
}
