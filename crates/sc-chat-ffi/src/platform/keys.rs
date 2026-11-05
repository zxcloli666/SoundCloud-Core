use std::sync::Arc;

use sc_chat::{ChatError, ChatResult, KeyName, KeyStorage, KeyStore, Zeroizing};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KeyFailure {
    #[error("{reason}")]
    Unavailable { reason: String },
    #[error("{reason}")]
    Denied { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for KeyFailure {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Unavailable {
            reason: error.reason,
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
        let secret = self.0.load(name).map_err(failed)?;
        Ok(secret.map(Zeroizing::new))
    }

    fn save(&self, name: KeyName, secret: &[u8]) -> ChatResult<()> {
        self.0.save(name, secret.to_vec()).map_err(failed)
    }

    fn delete(&self, name: KeyName) -> ChatResult<()> {
        self.0.delete(name).map_err(failed)
    }
}

fn failed(failure: KeyFailure) -> ChatError {
    match failure {
        KeyFailure::Denied { .. } => ChatError::KeychainDenied,
        KeyFailure::Unavailable { .. } => ChatError::Keychain,
    }
}
