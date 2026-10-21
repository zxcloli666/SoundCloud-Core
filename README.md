# SoundCloud Core

Shared Rust crates of the SoundCloud apps.

A crate lives here only if it is platform neutral: it builds for Windows, Linux, macOS, Android and iOS. Desktop-only crates (tray and the like) stay in Desktop, and the Tauri app keeps only the UI shell.

| Crate | What it is |
|---|---|
| [`sc-chat-wire`](crates/sc-chat-wire) | chat wire format and crypto, the protocol spec and the test vectors |
| [`sc-chat`](crates/sc-chat) | chat client core: sealed channel, store, sync, outbox, search |
| [`sc-chat-ffi`](crates/sc-chat-ffi) | UniFFI wrapper of `sc-chat`: an AAR for Android and an XCFramework for iOS, see [docs/chat-mobile.md](docs/chat-mobile.md) |

Apps pin a crate by its tag, `<crate>-v<version>`:

```toml
sc-chat = { git = "https://github.com/zxcloli666/SoundCloud-Core", tag = "sc-chat-v0.1.0" }
```

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

CI runs the tests on Linux, macOS and Windows and `cargo check` for Android (`aarch64`, `armv7`, `x86_64`) and iOS (device and simulator). `chat-mobile.yml` builds the Android AAR and the iOS XCFramework and keeps them as run artifacts.

The chat integration tests need the Backend dev stack from `scripts/chat-it.sh`, see [crates/sc-chat/tests/integration](crates/sc-chat/tests/integration/README.md).

## License

MIT
