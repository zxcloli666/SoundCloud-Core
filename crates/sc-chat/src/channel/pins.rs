use super::OckPin;

pub const DEV_OCK_ENV: &str = "SC_CHAT_DEV_OCK";

#[cfg(debug_assertions)]
pub fn pinned() -> Vec<OckPin> {
    match std::env::var(DEV_OCK_ENV)
        .ok()
        .and_then(|raw| dev_pin(&raw))
    {
        Some(pin) => vec![pin],
        None => sc_chat_wire::ock::OCK_PINS.to_vec(),
    }
}

#[cfg(not(debug_assertions))]
pub fn pinned() -> Vec<OckPin> {
    sc_chat_wire::ock::OCK_PINS.to_vec()
}

pub fn dev_pin(raw: &str) -> Option<OckPin> {
    let (version, key) = raw.trim().split_once(':')?;
    let version: u8 = version.parse().ok()?;
    if version == 0 || key.len() != 64 {
        return None;
    }
    let mut public = [0u8; 32];
    for (at, byte) in public.iter_mut().enumerate() {
        *byte = u8::from_str_radix(key.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some((version, public))
}
