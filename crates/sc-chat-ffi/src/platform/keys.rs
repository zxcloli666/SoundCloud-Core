use std::sync::Arc;

use sc_chat::{ChatError, ChatResult, KeyName, KeyStorage, KeyStore, Zeroizing};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KeyFailure {
    #[error("{message}")]
    Unavailable { message: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for KeyFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unavailable {
            message: error.reason,
        }
    }
}

#[uniffi::export(with_foreign)]
pub trait ChatKeys: Send + Sync {
    fn load(&self, name: KeyName) -> Result<Option<Vec<u8>>, KeyFailure>;

    fn save(&self, name: KeyName, secret: Vec<u8>) -> Result<(), KeyFailure>;

    fn delete(&self, name: KeyName) -> Result<(), KeyFailure>;
}

pub struct Keys(pub Arc<dyn ChatKeys>);

impl KeyStore for Keys {
    fn storage(&self) -> KeyStorage {
        KeyStorage::Os
    }

    fn load(&self, name: KeyName) -> ChatResult<Option<Zeroizing<Vec<u8>>>> {
        let secret = self.0.load(name).map_err(|_| ChatError::Keychain)?;
        Ok(secret.map(Zeroizing::new))
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        self.0
            .save(name, secret.to_vec())
            .map_err(|_| ChatError::Keychain)
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.0.delete(name).map_err(|_| ChatError::Keychain)
    }
}
