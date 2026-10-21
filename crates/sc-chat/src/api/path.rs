use std::fmt::Write;

pub fn segment(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub fn query(pairs: &[(&str, Option<String>)]) -> String {
    pairs
        .iter()
        .filter_map(|(key, value)| Some(format!("{key}={}", segment(value.as_deref()?))))
        .collect::<Vec<_>>()
        .join("&")
}
