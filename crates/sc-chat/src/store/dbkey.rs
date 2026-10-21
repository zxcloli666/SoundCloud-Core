use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use zeroize::Zeroizing;

use super::seal::{self, NONCE_LEN};
use super::{StoreKeys, meta};
use crate::contracts::{ChatError, ChatResult, KeyName, KeyStore};

const CHECK: &str = "key_check";
const CHECK_PLAIN: &[u8] = b"chat-store key check";

pub fn load_or_create(keys: &dyn KeyStore) -> ChatResult<(Zeroizing<[u8; 32]>, bool)> {
    if let Some(saved) = keys.load(KeyName::Database)?
        && let Ok(master) = <[u8; 32]>::try_from(saved.as_slice())
    {
        return Ok((Zeroizing::new(master), false));
    }
    let master = Zeroizing::new(sc_chat_wire::random_bytes::<32>().map_err(|_| ChatError::Crypto)?);
    keys.save(KeyName::Database, master.as_slice())?;
    Ok((master, true))
}

pub fn remove_files(path: &Path) -> ChatResult<()> {
    for file in [
        path.to_path_buf(),
        sibling(path, "-wal"),
        sibling(path, "-shm"),
    ] {
        match std::fs::remove_file(&file) {
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(ChatError::Store),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn owner_only(path: &Path) -> ChatResult<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| ChatError::Store)?;
    for file in [
        path.to_path_buf(),
        sibling(path, "-wal"),
        sibling(path, "-shm"),
    ] {
        match std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)) {
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(ChatError::Store),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn owner_only(_path: &Path) -> ChatResult<()> {
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

pub fn verify(conn: &Connection, keys: &StoreKeys) -> ChatResult<()> {
    let aad = seal::tagged_aad("check", &[]);
    match meta::get(conn, CHECK)? {
        Some(saved) if saved.len() > NONCE_LEN => {
            let (nonce, body) = saved.split_at(NONCE_LEN);
            let plain = seal::open(&keys.body, &aad, nonce, body)?;
            if plain.as_slice() == CHECK_PLAIN {
                Ok(())
            } else {
                Err(ChatError::Crypto)
            }
        }
        Some(_) => Err(ChatError::Crypto),
        None => {
            let sealed = seal::seal(&keys.body, &aad, CHECK_PLAIN)?;
            let mut value = sealed.nonce;
            value.extend(sealed.body);
            meta::set(conn, CHECK, &value)
        }
    }
}
