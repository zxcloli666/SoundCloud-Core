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
- `chat-it-s3`, SeaweedFS with the `chat-it` bucket as chat store 1;
- chat-api (`API_ROLE=rt`, debug build) on `127.0.0.1:3417` with AI moderation, the public chat and media uploads on;
- the full API (same binary, `AUTH_X=on`) on `127.0.0.1:3418` for `/auth/*`;
- `chat-it-front`, nginx on `127.0.0.1:6397` that sends `/auth/*` to the full API and the rest to chat-api. `CHAT_IT_ORIGIN` points at it.

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
| `request.rs` | under the Following policy: 1 h session rule, 3 posts before accept, decline closes the chat, accept moves it to the list; a dm from a followed sender is a chat at once, the first answer or accept marks it accepted |
| `block.rs` | block refuses posts as `chat_unavailable` and fails the queued copy, unblock lets new ones through |
| `report.rs` | evidence decrypts on the admin route, decrypt is audited, no plaintext in the chat-api log |
| `moderation.rs` | the AI judge rejects a harmless report, removes harassment, bans a repeat for 7 days, every step audited, the key and the text stay out of the log |
| `relogin.rs` | unbound session, bound to another device, sealed relogin of the current session registers with the stored bind, sealed pull and push links |
| `devices.rs` | a young device gets `chat_too_young` for remove, confirm and not me, the older one revokes it and ends its session |
| `history.rs` | a new device gets no old keys and reads only what came after it until an older device says it is me or 72 h pass; the older device gets the alert |
| `ttl.rs` | disappearing messages: timer rows, expiry on the server, `gone_msgs`, local expiry offline, timer off |
| `replay.rs` | a captured request sent again is `chat_replay` |
| `tiers.rs` | a dead direct route falls through to the podman relay |
| `outbox.rs` | an outbox written offline or with a lost answer is sent once after a restart |
| `public.rs` | the public chat is hidden without the `public_chat` cap, the first open joins; posts both ways to watchers, slow mode; the newcomer size cap matches `Newcomer::fits`; a mention and a reply ping only their target |
| `public_staff.rs` | the owner names a moderator who mutes with a reason, bans and lifts both; the moderation log |
| `backdrop.rs` | a DM preset needs STAR; a group image backdrop is uploaded, opened by a member and kept by an avatar change; Saved takes an image, the seventh change in 10 min is refused, a lapsed STAR hides the backdrop; the public owner sets a preset without STAR |
| `backdrop_report.rs` | a backdrop report waits for the admin with the image as evidence, the AI judge leaves it alone, an uphold clears the backdrop |

`public.rs`, `public_staff.rs` and the public test in `backdrop.rs` take turns on the one public chat. They log in its owner, user 1275950656, and drop his older devices and limits first.
