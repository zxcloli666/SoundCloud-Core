# Chat on Android and iOS

`sc-chat-ffi` is the chat core (`sc-chat`) for mobile apps, built with UniFFI. Sealing, crypto, the local store, sync and the outbox run in Rust. The app gives it three things: key storage, an HTTP transport and an event listener.

## Get the library

The `Chat mobile` workflow builds both packages on every push to `main` and on `sc-chat-ffi-v*` tags. Take them from the run of a tag:

```sh
gh run list --repo zxcloli666/SoundCloud-Core --workflow chat-mobile.yml --branch sc-chat-ffi-v0.2.5
gh run download <run id> --repo zxcloli666/SoundCloud-Core
```

| Artifact | Contents |
|---|---|
| `sc-chat-android` | `sc-chat-release.aar`: Kotlin bindings, `arm64-v8a`, `armeabi-v7a`, `x86_64`, minSdk 24 |
| `sc-chat-ios` | Swift package: `Package.swift`, `Sources/ScChat/ScChat.swift`, `ScChatFFI.xcframework` (device arm64, simulator arm64 and x86_64), iOS 15+ |

### Android

Put the AAR into `app/libs` and add it with its two runtime dependencies:

```kotlin
dependencies {
    implementation(files("libs/sc-chat-release.aar"))
    implementation("net.java.dev.jna:jna:5.18.1@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
}
```

Package `dev.scnative.chat`. The R8 keep rules come inside the AAR.

### iOS

Copy the `sc-chat-ios` folder into the repo, add it in Xcode with File > Add Package Dependencies > Add Local, link the `ScChat` library to the app target and `import ScChat`.

## What the app implements

| Interface | Job |
|---|---|
| `ChatKeys` | `load`, `save`, `delete` of four secrets by `KeyName`, see Storage |
| `ChatTransport` | `tiers()`: tiers to try, best first. `exchange(request)`: send one sealed envelope |
| `ChatListener` | `onEvent(event)`: status, sync, messages, typing, gone, device added, media uploads |

`exchange` rules:

- `POST` `request.envelope` with `Content-Type: application/octet-stream` to `https://api.scnative.dev/chat/x` for `DIRECT`, to the same path on a relay host for `RELAY`, through a pro node for `PRO`. List only the tiers the app can reach.
- Return the status, the body bytes and `Retry-After` in ms. Do not follow redirects.
- Throw `NetFailure.Timeout` after `request.timeoutMs`. Throw `NetFailure.Unreachable` on a connect error, on 421, on 502-504 without `Retry-After` and on a body over 2 MB. `Cancelled` only when the app cancels on purpose.

`onEvent` runs on a chat thread: post to the main thread, never block it.

## Calls in order

### Start

One client per process, kept for the app lifetime.

```kotlin
val client = ChatClient(
    ChatSetup(
        dataDir = File(context.filesDir, "chat").path,
        platform = "android",
        deviceName = Build.MODEL,
        appVersion = BuildConfig.VERSION_NAME,
    ),
    keys, transport, listener,
)
client.setSession(sessionId)
client.setVisible(true)
```

```swift
let client = try ChatClient(
    setup: ChatSetup(dataDir: chatDir.path, platform: "ios", deviceName: UIDevice.current.model, appVersion: version),
    keys: keys, transport: transport, listener: listener
)
client.setSession(sessionId: sessionId)
client.setVisible(visible: true)
```

- `setSession(id)`: the music session id. Call it on start, after every login (even with the same id) and with `null` on sign out. Sign out removes this chat device and wipes its store, its orphans and its keys.
- `setVisible(true)` in the foreground, `false` in the background. Visible means long polls and fast delivery, hidden means one sync a minute.
- `setFocus(conversationId)` while a thread is on screen, `setFocus(null)` when it closes.
- `networkChanged()` after Wi-Fi, mobile data or VPN changes.
- `retryKeychain()` when the user asks to try the keychain again; it does nothing outside `KEYCHAIN_DENIED` and `KEYCHAIN_UNAVAILABLE`, and a call while a try runs is dropped.

### Status

`client.status()` and `CoreEvent.Status` carry `ChatStatus`. Chat works when `phase` is `READY`.

| Phase | Do |
|---|---|
| `SIGNED_OUT` | no session, call `setSession` |
| `STARTING` | wait; `online` says whether a tier answers |
| `UNBOUND` | the session has no chat bind: sealed relogin, below |
| `BOUND_OTHER` | the session is bound to another device: sealed relogin |
| `REVOKED` | this device was removed, its data is gone: sealed relogin |
| `OFF` | chat is off for this account |
| `KEYCHAIN_DENIED` | the user refused the keychain: chat waits, no prompt comes back until `retryKeychain()` |
| `KEYCHAIN_UNAVAILABLE` | the keychain fails without a refusal: chat retries on its own, about 2 s at first, then at most a minute apart; `retryKeychain()` tries as soon as 2 s have passed since the last try |

`history.locked` and `history.unlocksAt`: a new device reads old chats only after an older device confirms it or after 72 h.

### Login (`seal_login`)

1. `loginBegin(LoginTarget.FRESH)` for a new sign-in, `loginBegin(LoginTarget.CURRENT)` to bind the current session (relogin). It answers `url` and `loginRequestId`.
2. Open `url` in Custom Tabs or `ASWebAuthenticationSession`.
3. Every 2 s call `loginPoll(loginRequestId)` while `status` is `PENDING`; `step` shows progress.
4. `COMPLETED`: store `sessionId` as the app session and call `setSession(sessionId)`. `FAILED` or `EXPIRED`: show `error`, start again.

### Link a second device

The QR code is `scd://link?token=<claimToken>&mode=<pull|push>`, the same as on desktop.

| Mode | New device, no session | Signed-in device |
|---|---|---|
| pull | `linkCreate(PULL)`, show the QR, poll `linkPoll(linkRequestId)` every 2 s until `CLAIMED`, then `setSession(sessionId)` | scan, `linkClaim(PULL, token)` |
| push | scan, `linkClaim(PUSH, token)`, then `setSession(claimed.sessionId)` | `linkCreate(PUSH)`, show the QR, poll `linkPoll` until `CLAIMED` |

### Chats

| Call | Notes |
|---|---|
| `conversations(ListQuery(filter, cursor, limit))` | the list from the local store, `nextCursor` for the next page |
| `openThread(id)` | last 50 messages with pending ones, read marks, pins, draft |
| `history(id, HistoryQuery(before = oldestSeq, after = null, limit))` | older messages; goes to the server when the store runs out |
| `imagesBefore(id, before, limit)` | the gallery: stored posts with images, newest first, older than `before`; `more` says whether another page exists |
| `send(SendInput(conversationId, clientId, text, replyTo, embeds, forward))` | answers the pending message at once; the sent copy comes back in `CoreEvent.Messages` with `seq` and `state` |
| `markRead(id, seq)` | the newest `seq` on screen |
| `createConversation(CreateConversation(kind, members, title))` | `DIRECT` with one user id, or `GROUP` |
| `setTtl(id, ttlS)` | disappearing messages: `3600`, `86400`, `604800`, `null` turns it off. Expired messages leave the store even offline, as `CoreEvent.Gone` |
| `report(ReportInput(id, seqFrom, seqTo, reason, comment))` | comment up to 500 chars |
| `openPublic()` | opens the public chat: `PublicOpened(conversation, online, mute, newcomer)`. `mute` is the caller's moderator mute (`ModeratorMute(until, reason)`), not the notification mute in `me`. `newcomer` (`Newcomer(until, bodyMax)`) is the first 10 minutes after the first post: a post whose padded body is over `bodyMax` bytes is refused with `SendLimited`; `until` is `null` before the first post. `postBodyLen(text)` gives the padded body of a plain text post for a counter |

Keep the list and the open thread in sync with `CoreEvent.Sync` (changed conversations, read marks, request count), `Messages`, `Typing` and `Gone`.

### Images, avatars and backdrops

Core seals every file. The app keeps the sealed bytes in its own files and does the presigned `PUT` and `GET` itself, straight to the given `url` with exactly the given `headers`, never through `ChatTransport`.

| Call | Notes |
|---|---|
| `mediaConfig()` | `uploads`, `star`, `quota`, `used`, `limits`; no uploads until the first config |
| `prepareImage(source)` | WebP up to 1600 px, sealed `image` and `thumb`, run off the main thread |
| `prepareAvatar(source)` | group avatar: 512 px, sealed size up to 64 KiB, no thumb |
| `prepareBackdrop(source)` | chat backdrop: 1920 px, sealed size up to 600 KiB, no thumb, first frame only |

`prepare*` answers `PreparedMedia`: the sealed `image` (and `thumb`), `w`, `h`, `thumbhash` (the placeholder) and `seal`, an opaque handle that keeps the key in Core. Write `image` and `thumb` to files, then queue them:

1. `enqueueUpload(NewMediaUpload(album, position, conversationId, imageRef, thumbRef, seal))`: `thumbRef` exactly for an image. A backdrop takes the id of the chat it is for (the room chat id for a room). Answers `MediaUploadRow`.
2. `reserveUpload(id)`: `PutTargets(expiresAt, image, thumb)`. Reserve again after `expiresAt`; the same row keeps its ids.
3. `PUT` the image file. Report `uploadProgress(id, IMAGE, partsSent, stageId)` while it goes and `uploadObjectDone(id, IMAGE)` after it. An image then sends its thumb the same way (`THUMB`).
4. `commitUpload(id)`. `MediaMissing` puts the row back on the missing object: `PUT` it again and commit. `NotFound` means the reserve expired: the row is `QUEUED` again. When the row was removed while the commit ran, `NotFound` too, and Core deletes the server copy.
5. On a network error `uploadFailed(id, retryable = true)` and retry; `retryable = false` ends it as `FAILED`. `uploadRestart(id)` starts a row over, `uploadRedo(id, object)` sends one object again, `removeAlbumUploads(album)` cancels an album and `removeUpload(id)` one row of it (delete the files they answer; `null` when the row is already gone).

`uploads()`, `upload(id)`, `albumUploads(album)` and `nextUpload()` read the queue; it survives app restarts. `CoreEvent.MediaUploads` carries every change, `removed` the cancelled ids. After `sendAlbum`, `setGroupAvatar` or `setBackdrop` answers, the message holds the keys: call `removeAlbumUploads(album)` or `removeUpload(id)` and keep the files only as a cache.

| Call | Notes |
|---|---|
| `sendAlbum(SendInput, album)` | posts the album once every upload of it is `COMMITTED` (`NotReady` before); up to 10 images, 4 in a room. `Invalid` for a `FAILED` row, a non-image row or a row of another chat |
| `setGroupAvatar(id, AvatarChange.Set(uploadId))` | a committed avatar upload of this group; `Clear` drops it. Rides a meta message |
| `setBackdrop(conversationId, BackdropChange.Set(uploadId))` | any chat: a direct chat (either member), a group (owner or admin), Saved, the public chat (owner or moderators) or a room (host). Sets the backdrop on the server, then queues the meta message with the key: a group meta keeps the avatar, other kinds carry the backdrop alone. `Clear` removes it; a cleared room or group meta is queued only while the known meta still holds an image, other kinds queue nothing. Answers the queued meta or `null`. `BackdropStarRequired` without STAR (presets in the public chat need none), `NotMember` without the role, `Unavailable` for a direct chat whose peer is not active, `RequestLimit` after 6 changes per chat in 10 min, `BackdropCooldown(until)` in a room whose image was taken down by reports |
| `setBackdropPreset(conversationId, presetId)` | a bundled preset for any chat; the same meta rule as `Clear` |
| `setRoomBackdrop(roomChatId, change)`, `setRoomPreset(roomChatId, presetId)` | the same for a room only; `Invalid` for another kind, `NotFound` when its room code is not known yet |
| `reportBackdrop(BackdropReport(conversationId, mediaId, reason, comment))` | a member reports the backdrop on screen in any chat, never its setter; comment up to 500 chars. `MediaMissing` means the chat shows another backdrop now |
| `mediaInfo(conversationId, mediaId)` | size, `w`, `h`, `thumbhash` and the thumb id of an album image, a group avatar or a backdrop |
| `mediaUrls(conversationId, ids)` | presigned `GET` urls, 15 min; `DELETED` shows "Картинка удалена", `LOST` "Картинка недоступна" |
| `openImage(conversationId, mediaId, ciphertext)` | checks and decrypts the downloaded bytes: `OpenedMedia(bytes, mime, w, h)`, JPEG or PNG |
| `openThumb(conversationId, mediaId, ciphertext)` | the same for the thumb of an image (`mediaId` is the image id) |
| `mediaUsage()`, `mediaMine(MineQuery)` | storage: quota, per chat, own files |
| `deleteMedia(MediaSelection)`, `restoreMedia(MediaSelection)` | delete for everyone in every chat; restore within 15 s |

`ConversationSummary.backdrop` names the backdrop of a direct chat, a group, Saved or the public chat: `ConversationBackdrop(look, by)`, `look` a `RoomBackdrop.Preset(id)` or `RoomBackdrop.Image(mediaId)`, `by` the user who set it; `null` when there is none or the setter lost STAR. Resolve an image with `mediaInfo(conversationId, mediaId)` as for a room; `null` means the meta with the key has not arrived yet. Old servers and unknown kinds read as `null`.

The room view (`backdrop` in the room state) decides what to draw. For `{"kind": "image", "mediaId"}` resolve it with `mediaInfo(roomChatId, mediaId)`, fetch it with `mediaUrls(roomChatId, [mediaId])` and open it with `openImage`; `null` from `mediaInfo` means the key has not arrived: draw the cover. The server can drop a backdrop without a new room meta, so draw only what the room state names; a room summary never carries `avatarMediaId`. When Core reads a new room meta, `CoreEvent.Sync` brings the room summary: resolve again then. A 404 on a `GET` asks `mediaUrls` once more, then shows the placeholder.

### Devices

| Call | Notes |
|---|---|
| `devices()` | `current` is this device, `actions` says which buttons to show |
| `confirmDevice(id)` | "it is me": the new device gets old history |
| `deviceNotMe(id)` | revokes it |
| `removeDevice(id)` | removing the current one logs out |
| `logOut()` | removes this device, wipes the store and the keys; chat stays `UNBOUND` until a relogin |

`CoreEvent.DeviceAdded` asks the user to confirm a new device.

### Errors

Every async call throws `ChatException` in Kotlin and `ChatError` in Swift. The usual ones: `NotReady` (phase is not `READY`), `Offline`, `Busy(retryMs)`, `SlowMode(retryMs)`, `Muted(until)`, `TooYoung`, `Invalid`, `NotFound`. Media adds `QuotaExceeded(used, quota)`, `MediaStarRequired`, `RoomStarRequired`, `BackdropCooldown(until)`, `MediaUploadsPaused` and `MediaMissing(ids)`. A malformed answer is `ProtocolException` in Kotlin and `ProtocolError` in Swift.

## Storage

- The store is SQLite at `<dataDir>/chat.db` with `-wal` and `-shm`, mode 0600. Use app-private storage (Android `filesDir`, iOS Application Support). Exclude the whole `dataDir` from OS backups, not single files, since Core adds files next to `chat.db`: Android leaves the folder out in `dataExtractionRules` and `fullBackupContent`, iOS sets `isExcludedFromBackup` on the folder.
- If the database key is gone or no longer opens `chat.db`, Core moves the old file aside to `chat.db.orphan-<unix s>` (with its `-wal` and `-shm`) and starts a fresh store; the newest 2 orphans stay. A store with no chats left is removed instead. A `-wal` or `-shm` found without its `chat.db` is parked under a fresh orphan name of its own, never next to an older orphan, and the newest 2 of those stay too. Only files named exactly `chat.db.orphan-<digits>` with an optional `-wal` or `-shm` count as orphans. Sign out, log out and a revoke delete every orphan, and also `chat.db` when no store is open (keychain denied or unavailable); the keys are erased even when a file cannot be removed.
- Message bodies, previews, titles, drafts, people and conversation keys are sealed with XChaCha20-Poly1305 under subkeys of `KeyName.DATABASE`; the search index uses keyed tokens. Ids, seqs and timestamps are plain.
- `ChatKeys` holds four secrets: `DATABASE` (32 random bytes, the store key), `DEVICE_ID` (16 bytes), `DEVICE_AUTH` (Ed25519 seed) and `DEVICE_BOX` (X25519 secret). Android: encrypt them with an Android Keystore key. iOS: Keychain with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`. They must never leave the device.
- `ChatKeys` throws `KeyFailure.Denied` when the user refuses access (Android biometric cancel, iOS user cancel) and `KeyFailure.Unavailable` for anything else. A denial never drops the store or the device keys.
- A lost `DATABASE` key drops the store, sync fills it again. Lost device keys make a new device: relogin, and old history waits for a confirm.
