pub const TEXT_MAX: usize = 120;
pub const ROOM_TITLE_MAX: usize = 48;
pub const URN_MAX: usize = 128;
pub const SECRET_MAX: usize = 64;
pub const ROOM_CODE_LEN: usize = 6;

const URN_PREFIX: &str = "soundcloud:";
const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const ACCESS: [&str; 3] = ["playable", "preview", "blocked"];

pub fn text(raw: &str, max: usize) -> String {
    let kept: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control() && !is_bidi_control(*c))
        .take(max)
        .collect();
    kept.trim().to_owned()
}

pub fn optional_text(raw: Option<&str>, max: usize) -> Option<String> {
    raw.map(|raw| text(raw, max))
        .filter(|text| !text.is_empty())
}

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

pub fn urn(raw: &str) -> Option<String> {
    let urn = raw.trim();
    let safe = urn.len() <= URN_MAX
        && urn.len() > URN_PREFIX.len()
        && urn.starts_with(URN_PREFIX)
        && urn
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.'));
    safe.then(|| urn.to_owned())
}

pub fn secret(raw: Option<&str>) -> Option<String> {
    let token = raw?.trim();
    let safe = !token.is_empty()
        && token.len() <= SECRET_MAX
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'));
    safe.then(|| token.to_owned())
}

pub fn access(raw: Option<&str>) -> Option<String> {
    let access = raw?.trim();
    ACCESS.contains(&access).then(|| access.to_owned())
}

pub fn room_code(raw: &str) -> Option<String> {
    let code: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let valid =
        code.len() == ROOM_CODE_LEN && code.bytes().all(|b| ROOM_CODE_ALPHABET.contains(&b));
    valid.then_some(code)
}
