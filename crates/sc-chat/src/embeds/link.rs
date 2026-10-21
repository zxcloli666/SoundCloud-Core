use crate::contracts::EMBED_ARTWORK_HOST_SUFFIX;

pub const URL_MAX: usize = 512;

const SCHEME: &str = "https://";
const ARTWORK_HOST: &str = "sndcdn.com";
const SOUNDCLOUD_HOSTS: [&str; 4] = [
    "soundcloud.com",
    "www.soundcloud.com",
    "m.soundcloud.com",
    "on.soundcloud.com",
];

pub fn artwork(raw: Option<&str>) -> Option<String> {
    let url = raw?.trim();
    let host = https_host(url)?;
    let trusted = host == ARTWORK_HOST
        || host
            .strip_suffix(EMBED_ARTWORK_HOST_SUFFIX)
            .is_some_and(|rest| !rest.is_empty());
    trusted.then(|| url.to_owned())
}

pub fn soundcloud(raw: &str) -> Option<String> {
    let url = raw.trim();
    let host = https_host(url)?;
    SOUNDCLOUD_HOSTS.contains(&host).then(|| url.to_owned())
}

fn https_host(url: &str) -> Option<&str> {
    if url.len() > URL_MAX || !url.bytes().all(safe_byte) {
        return None;
    }
    let rest = url.strip_prefix(SCHEME)?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    let valid = !host.is_empty()
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'));
    valid.then_some(host)
}

fn safe_byte(b: u8) -> bool {
    b.is_ascii_graphic() && !matches!(b, b'\\' | b'"' | b'<' | b'>' | b'`')
}
