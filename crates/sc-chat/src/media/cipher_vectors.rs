use image::{DynamicImage, Rgb, RgbImage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{BLOCK, NONCE_LEN, Role, open_blob, seal_at, sealed_len};
use crate::media::ladder;

const FIXTURE: &str = include_str!("../../vectors/media_seal.json");
const BOUNDARY_PLAIN: usize = BLOCK - 1 - 16;

struct Case {
    name: &'static str,
    role: Role,
    plain: Vec<u8>,
}

fn key_of(name: &str) -> [u8; 32] {
    Sha256::digest(format!("sc-chat media vector key/{name}")).into()
}

fn nonce_of(name: &str) -> [u8; NONCE_LEN] {
    let digest = Sha256::digest(format!("sc-chat media vector nonce/{name}"));
    digest[..NONCE_LEN].try_into().unwrap()
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Image => "image",
        Role::Thumb => "thumb",
    }
}

fn role_of(name: &str) -> Role {
    match name {
        "image" => Role::Image,
        "thumb" => Role::Thumb,
        other => panic!("unknown role {other}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn pattern(len: usize, seed: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 32);
    let mut counter = 0u32;
    while out.len() < len {
        let mut hasher = Sha256::new();
        hasher.update(seed.as_bytes());
        hasher.update(counter.to_be_bytes());
        out.extend_from_slice(&hasher.finalize());
        counter += 1;
    }
    out.truncate(len);
    out
}

fn picture(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        let wave = ((x * 7 + y * 3) % 64) as u8;
        Rgb([
            (x * 255 / w) as u8 ^ wave,
            (y * 255 / h) as u8,
            ((x + y) * 3) as u8 ^ (wave << 2),
        ])
    }))
}

fn cases() -> Vec<Case> {
    let small = ladder::encode(&picture(24, 16), &ladder::THUMB).unwrap();
    let album = picture(400, 300);
    let image = ladder::encode(&album, &ladder::IMAGE).unwrap();
    let thumb = ladder::encode(&album, &ladder::THUMB).unwrap();
    vec![
        Case {
            name: "small",
            role: Role::Image,
            plain: small.webp,
        },
        Case {
            name: "boundary",
            role: Role::Image,
            plain: pattern(BOUNDARY_PLAIN, "boundary"),
        },
        Case {
            name: "boundary-spill",
            role: Role::Image,
            plain: pattern(BOUNDARY_PLAIN + 1, "boundary-spill"),
        },
        Case {
            name: "album-image",
            role: Role::Image,
            plain: image.webp,
        },
        Case {
            name: "album-thumb",
            role: Role::Thumb,
            plain: thumb.webp,
        },
    ]
}

fn sealed(name: &str, role: Role, plain: &[u8]) -> Vec<u8> {
    seal_at(role, Zeroizing::new(key_of(name)), &nonce_of(name), plain)
        .unwrap()
        .ciphertext
}

fn vector(case: &Case) -> Value {
    let ciphertext = sealed(case.name, case.role, &case.plain);
    json!({
        "name": case.name,
        "role": role_name(case.role),
        "aad": hex(&case.role.aad()),
        "key": hex(&key_of(case.name)),
        "nonce": hex(&nonce_of(case.name)),
        "plain_len": case.plain.len(),
        "sealed_len": ciphertext.len(),
        "plain": hex(&case.plain),
        "sha256_plain": hex(&Sha256::digest(&case.plain)),
        "sha256_cipher": hex(&Sha256::digest(&ciphertext)),
        "ciphertext": hex(&ciphertext),
    })
}

fn field<'a>(vector: &'a Value, name: &str) -> &'a str {
    vector[name].as_str().unwrap()
}

#[test]
#[ignore]
fn write_the_media_seal_vectors() {
    let fixture = json!({
        "format": [
            "XChaCha20-Poly1305, 32-byte key and 24-byte nonce per object; the nonce is stored apart from the ciphertext (Attachment.nonce, Thumb.nonce), the key in Attachment.key and Thumb.key.",
            "Plaintext is padded before sealing: plain || 0x80 || zero bytes, sealed_len - 16 bytes in all, so the ciphertext with its 16-byte tag is a whole number of 4096-byte blocks.",
            "Open: refuse a ciphertext that is empty or not a multiple of 4096, decrypt, drop trailing zero bytes, the last byte left must be 0x80 and goes too.",
            "AAD is \"chat-store/media/v1\\n\" followed by a 4-byte big-endian length and the role label: \"image\" for images, avatars and room backdrops, \"thumb\" for thumbs.",
            "sha256_cipher is Attachment.sha256_cipher and sha256_plain is Attachment.sha256_plain; a thumb has neither in the envelope.",
            "Vector key = SHA-256(\"sc-chat media vector key/\" + name), nonce = first 24 bytes of SHA-256(\"sc-chat media vector nonce/\" + name). Real objects use random keys and nonces."
        ],
        "vectors": cases().iter().map(vector).collect::<Vec<_>>(),
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vectors/media_seal.json");
    let text = serde_json::to_string_pretty(&fixture).unwrap();
    std::fs::write(path, format!("{text}\n")).unwrap();
}

#[test]
fn the_fixture_is_what_the_real_seal_makes() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let vectors = fixture["vectors"].as_array().unwrap();
    let names: Vec<&str> = vectors.iter().map(|vector| field(vector, "name")).collect();
    assert_eq!(
        names,
        [
            "small",
            "boundary",
            "boundary-spill",
            "album-image",
            "album-thumb"
        ]
    );
    for vector in vectors {
        let name = field(vector, "name");
        let role = role_of(field(vector, "role"));
        let plain = unhex(field(vector, "plain"));
        let ciphertext = unhex(field(vector, "ciphertext"));
        assert_eq!(unhex(field(vector, "key")), key_of(name), "{name}");
        assert_eq!(unhex(field(vector, "nonce")), nonce_of(name), "{name}");
        assert_eq!(unhex(field(vector, "aad")), role.aad(), "{name}");
        assert_eq!(vector["plain_len"].as_u64().unwrap() as usize, plain.len());
        assert_eq!(sealed_len(plain.len()), ciphertext.len(), "{name}");
        assert_eq!(
            vector["sealed_len"].as_u64().unwrap() as usize,
            ciphertext.len()
        );
        assert_eq!(sealed(name, role, &plain), ciphertext, "{name}");
        assert_eq!(
            unhex(field(vector, "sha256_plain")),
            Sha256::digest(&plain)[..]
        );
        assert_eq!(
            unhex(field(vector, "sha256_cipher")),
            Sha256::digest(&ciphertext)[..]
        );
        let opened = open_blob(role, &key_of(name), &nonce_of(name), &ciphertext).unwrap();
        assert_eq!(opened.as_slice(), plain.as_slice(), "{name}");
    }
}

#[test]
fn the_fixture_covers_the_block_edges() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let sizes: Vec<(usize, usize)> = fixture["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|vector| {
            (
                vector["plain_len"].as_u64().unwrap() as usize,
                vector["sealed_len"].as_u64().unwrap() as usize,
            )
        })
        .collect();
    assert!(sizes[0].0 < 1024 && sizes[0].1 == BLOCK);
    assert_eq!(sizes[1], (BOUNDARY_PLAIN, BLOCK));
    assert_eq!(sizes[2], (BOUNDARY_PLAIN + 1, 2 * BLOCK));
    assert!(sizes[3].1 >= 3 * BLOCK);
    assert_eq!(sizes[4].1 % BLOCK, 0);
}

#[test]
fn a_fixture_ciphertext_refuses_a_flip_the_other_role_and_a_cut() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for vector in fixture["vectors"].as_array().unwrap() {
        let name = field(vector, "name");
        let role = role_of(field(vector, "role"));
        let other = match role {
            Role::Image => Role::Thumb,
            Role::Thumb => Role::Image,
        };
        let ciphertext = unhex(field(vector, "ciphertext"));
        let (key, nonce) = (key_of(name), nonce_of(name));
        let mut flipped = ciphertext.clone();
        flipped[ciphertext.len() / 2] ^= 1;
        assert!(open_blob(role, &key, &nonce, &flipped).is_err(), "{name}");
        assert!(
            open_blob(other, &key, &nonce, &ciphertext).is_err(),
            "{name}"
        );
        assert!(
            open_blob(role, &key, &nonce, &ciphertext[..ciphertext.len() - BLOCK]).is_err(),
            "{name}"
        );
    }
}
