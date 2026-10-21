use std::sync::{Mutex, MutexGuard, PoisonError};

use sc_chat_wire::proto::{Bind, Inner};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::message::Ck;
use crate::contracts::{ChatError, ChatResult, ConvId, DeviceId, KeyName, KeyStore};

static KEYCHAIN: Mutex<()> = Mutex::new(());

pub struct DeviceKeys {
    auth_seed: Zeroizing<[u8; 32]>,
    box_secret: Zeroizing<[u8; 32]>,
}

impl DeviceKeys {
    pub fn from_seeds(auth_seed: [u8; 32], box_secret: [u8; 32]) -> Self {
        Self {
            auth_seed: Zeroizing::new(auth_seed),
            box_secret: Zeroizing::new(box_secret),
        }
    }

    pub fn generate() -> ChatResult<Self> {
        Ok(Self {
            auth_seed: Zeroizing::new(sc_chat_wire::random_bytes()?),
            box_secret: Zeroizing::new(sc_chat_wire::random_bytes()?),
        })
    }

    pub fn exclusive() -> MutexGuard<'static, ()> {
        KEYCHAIN.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn load_or_create(store: &dyn KeyStore) -> ChatResult<(DeviceId, Self)> {
        let _one_at_a_time = Self::exclusive();
        match Self::load(store)? {
            Some(found) => Ok(found),
            None => Self::create(store),
        }
    }

    pub fn erase(store: &dyn KeyStore) -> ChatResult<()> {
        let _one_at_a_time = Self::exclusive();
        store.delete(KeyName::DeviceId)?;
        store.delete(KeyName::DeviceAuth)?;
        store.delete(KeyName::DeviceBox)
    }

    fn load(store: &dyn KeyStore) -> ChatResult<Option<(DeviceId, Self)>> {
        let Some(device_id) = store.load(KeyName::DeviceId)? else {
            return Ok(None);
        };
        let auth_seed = store.load(KeyName::DeviceAuth)?;
        let box_secret = store.load(KeyName::DeviceBox)?;
        let device_id = Uuid::from_slice(&device_id).ok().filter(|id| !id.is_nil());
        let (Some(device_id), Some(auth_seed), Some(box_secret)) =
            (device_id, secret(auth_seed), secret(box_secret))
        else {
            return Ok(None);
        };
        Ok(Some((
            device_id,
            Self {
                auth_seed,
                box_secret,
            },
        )))
    }

    fn create(store: &dyn KeyStore) -> ChatResult<(DeviceId, Self)> {
        let device_id = Uuid::new_v4();
        let keys = Self::generate()?;
        store.save(KeyName::DeviceBox, keys.box_secret.as_slice())?;
        store.save(KeyName::DeviceAuth, keys.auth_seed.as_slice())?;
        store.save(KeyName::DeviceId, device_id.as_bytes())?;
        Ok((device_id, keys))
    }

    pub fn auth_public(&self) -> [u8; 32] {
        sc_chat_wire::sign::public_key(&self.auth_seed)
    }

    pub fn box_public(&self) -> ChatResult<[u8; 32]> {
        Ok(sc_chat_wire::ck::box_public(&self.box_secret)?)
    }

    pub fn bind(&self, device_id: DeviceId, ts_ms: u64) -> ChatResult<Bind> {
        let box_pub = self.box_public()?;
        Ok(sc_chat_wire::bind::make_bind(
            &device_id.to_string(),
            &self.auth_seed,
            &box_pub,
            ts_ms,
        ))
    }

    pub fn sign(&self, inner: &mut Inner, enc: &[u8; 32]) -> ChatResult<()> {
        let signed = [
            inner.method.as_str(),
            inner.path.as_str(),
            inner.query.as_str(),
            inner.session_id.as_deref().unwrap_or_default(),
            inner.device_id.as_deref().unwrap_or_default(),
        ];
        if signed.iter().any(|field| field.contains('\n')) {
            return Err(ChatError::Invalid);
        }
        sc_chat_wire::sign::sign_request(&self.auth_seed, inner, enc);
        Ok(())
    }

    pub fn open_ck(
        &self,
        conversation_id: ConvId,
        epoch: u32,
        device_id: DeviceId,
        sealed: &[u8],
    ) -> ChatResult<Ck> {
        let ck = sc_chat_wire::ck::open_for_device(
            &self.box_secret,
            conversation_id.as_bytes(),
            epoch,
            device_id.as_bytes(),
            sealed,
        )?;
        Ok(Ck::from_bytes(ck))
    }
}

fn secret(stored: Option<Zeroizing<Vec<u8>>>) -> Option<Zeroizing<[u8; 32]>> {
    let stored = stored?;
    let mut secret = Zeroizing::new([0u8; 32]);
    if stored.len() != secret.len() {
        return None;
    }
    secret.copy_from_slice(&stored);
    Some(secret)
}
