#![cfg(feature = "bindgen")]

use std::path::{Path, PathBuf};
use std::process::Command;

const CALLS: [&str; 34] = [
    "mediaConfig",
    "prepareImage",
    "prepareAvatar",
    "prepareBackdrop",
    "enqueueUpload",
    "reserveUpload",
    "uploadProgress",
    "uploadObjectDone",
    "uploadRedo",
    "commitUpload",
    "uploadFailed",
    "uploadRestart",
    "removeAlbumUploads",
    "removeUpload",
    "upload",
    "uploads",
    "albumUploads",
    "nextUpload",
    "sendAlbum",
    "mediaUrls",
    "mediaInfo",
    "openImage",
    "openThumb",
    "mediaUsage",
    "mediaMine",
    "deleteMedia",
    "restoreMedia",
    "setGroupAvatar",
    "reportBackdrop",
    "setRoomBackdrop",
    "setRoomPreset",
    "imagesBefore",
    "retryKeychain",
    "networkChanged",
];

fn library() -> PathBuf {
    let bindgen = Path::new(env!("CARGO_BIN_EXE_uniffi-bindgen"));
    let name = format!(
        "{}sc_chat_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let dir = bindgen.parent().unwrap();
    [dir.join(&name), dir.join("deps").join(&name)]
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("{name} is built next to {}", dir.display()))
}

fn generated(language: &str) -> String {
    let out = std::env::temp_dir().join(format!("sc-chat-ffi-{language}-{}", uuid::Uuid::new_v4()));
    let status = Command::new(env!("CARGO_BIN_EXE_uniffi-bindgen"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["generate", "--library"])
        .arg(library())
        .args(["--language", language, "--no-format", "--out-dir"])
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "uniffi-bindgen {language}");
    let text = sources(&out);
    let _ = std::fs::remove_dir_all(&out);
    text
}

fn sources(dir: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            text.push_str(&sources(&path));
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("kt" | "swift")
        ) {
            text.push_str(&std::fs::read_to_string(&path).unwrap());
        }
    }
    text
}

fn missing(text: &str, shape: impl Fn(&str) -> String) -> Vec<&'static str> {
    CALLS
        .into_iter()
        .filter(|call| !text.contains(&shape(call)))
        .collect()
}

#[test]
fn the_kotlin_bindings_carry_every_media_call() {
    let kotlin = generated("kotlin");
    assert_eq!(
        missing(&kotlin, |call| format!("fun `{call}`(")),
        Vec::<&str>::new()
    );
    assert!(kotlin.contains("class KeychainDenied("));
    assert!(kotlin.contains("KEYCHAIN_DENIED"));
    assert!(!kotlin.contains("class Unimplemented("));
}

#[test]
fn the_swift_bindings_carry_every_media_call() {
    let swift = generated("swift");
    assert_eq!(
        missing(&swift, |call| format!("func {call}(")),
        Vec::<&str>::new()
    );
    assert!(swift.contains("case keychainDenied"));
    assert!(!swift.contains("case unimplemented"));
}
