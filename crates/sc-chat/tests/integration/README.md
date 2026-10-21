# Integration scenarios (1.T)

Real sc-chat clients against a dev chat-api, through the sealed channel. Every test is `#[ignore]`, plain `cargo test` skips them.
The tests write sessions straight into the database and age rows by hand, so point them only at a throwaway stack.

## Stack

Backend `scripts/chat-it.sh` builds it in podman and on the host:

- a fresh database (`CHAT_IT_DB`, default `scd_chat_it`) in the `scd-pg` container with every core migration and the sqlx history;
- `chat-it-redis-rt` and `chat-it-redis` (music);
- dev keys: `ock-1` is the test OCK from `sc-chat-wire/vectors/request.json` of the tag Backend pins, the KEKs, the admin token and `mod-ai-key` are random;
- `chat-it-ai`, a fake OpenAI-compatible judge (`scripts/chat-it/fake-ai.py`) that checks the key and the strict schema;
- `chat-it-relay`, nginx that forwards `/chat/x` to chat-api like a relay;
- chat-api (`API_ROLE=rt`, debug build) on `127.0.0.1:3417` with AI moderation on.

```sh
cd Backend && scripts/chat-it.sh up > /tmp/chat-it.env
```

## Run

From the Core checkout:

```sh
source /tmp/chat-it.env
cargo test -p sc-chat --test integration -- --ignored
```

`SC_CHAT_DEV_OCK` is read only by debug builds. `scripts/chat-it.sh down` drops the database, the containers and the keys.

## Scenarios

| File | Checks |
|---|---|
| `dm.rs` | DM both ways, `mine`, delivered and read ticks on both sides |
| `group.rs` | fan-out, a promotion row, a kick purges the chat on the kicked device, a leave |
| `room.rs` | rename, mute, kick, ban, handoff, end with server and local purge; a quiet room is reaped after 10 min |
| `request.rs` | 1 h session rule, 3 posts before accept, decline closes the chat, accept moves it to the list |
| `block.rs` | block refuses posts as `chat_unavailable` and fails the queued copy, unblock lets new ones through |
| `report.rs` | evidence decrypts on the admin route, decrypt is audited, no plaintext in the chat-api log |
| `moderation.rs` | the AI judge rejects a harmless report, removes harassment, bans a repeat for 7 days, every step audited, the key and the text stay out of the log |
| `relogin.rs` | unbound session, bound to another device, sealed relogin of the current session registers with the stored bind, sealed pull and push links |
| `devices.rs` | a young device gets `chat_too_young` for remove, confirm and not me, the older one revokes it |
| `history.rs` | a new device gets no old keys and reads only what came after it until an older device says it is me or 72 h pass; the older device gets the alert |
| `ttl.rs` | disappearing messages: timer rows, expiry on the server, `gone_msgs`, local expiry offline, timer off |
| `replay.rs` | a captured request sent again is `chat_replay` |
| `tiers.rs` | a dead direct route falls through to the podman relay |
| `outbox.rs` | an outbox written offline or with a lost answer is sent once after a restart |
