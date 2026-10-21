use rusqlite::Connection;

pub const VERSION: i64 = 2;

const V1: &str = "
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
) WITHOUT ROWID;

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    body BLOB NOT NULL
) WITHOUT ROWID;

CREATE TABLE conversations (
    id BLOB PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('direct', 'group', 'room', 'self')),
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    state TEXT NOT NULL CHECK (state IN ('active', 'invited', 'left', 'kicked', 'banned')),
    accepted INTEGER NOT NULL DEFAULT 0,
    member_count INTEGER NOT NULL DEFAULT 0,
    peer TEXT,
    room_code TEXT,
    last_seq INTEGER NOT NULL DEFAULT 0,
    last_at INTEGER NOT NULL DEFAULT 0,
    last_read_seq INTEGER NOT NULL DEFAULT 0,
    hidden_before_seq INTEGER NOT NULL DEFAULT 0,
    key_epoch INTEGER NOT NULL DEFAULT 0,
    unread INTEGER NOT NULL DEFAULT 0,
    mentions INTEGER NOT NULL DEFAULT 0,
    muted_until INTEGER,
    notify TEXT NOT NULL DEFAULT 'all' CHECK (notify IN ('all', 'mentions', 'none')),
    pinned_at INTEGER,
    archived INTEGER NOT NULL DEFAULT 0,
    ttl_s INTEGER,
    history_locked INTEGER NOT NULL DEFAULT 0,
    meta_epoch INTEGER,
    meta_nonce BLOB,
    meta BLOB
) WITHOUT ROWID;

CREATE INDEX conversations_recent ON conversations (last_at DESC);

CREATE TABLE members (
    conversation_id BLOB NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member')),
    state TEXT NOT NULL CHECK (state IN ('active', 'invited', 'left', 'kicked', 'banned')),
    PRIMARY KEY (conversation_id, user_id)
) WITHOUT ROWID;

CREATE TABLE marks (
    conversation_id BLOB NOT NULL,
    user_id TEXT NOT NULL,
    read_seq INTEGER NOT NULL DEFAULT 0,
    delivered_seq INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (conversation_id, user_id)
) WITHOUT ROWID;

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    conversation_id BLOB NOT NULL,
    seq INTEGER NOT NULL,
    client_id BLOB NOT NULL,
    sender TEXT NOT NULL,
    sender_device BLOB,
    class TEXT NOT NULL CHECK (class IN ('post', 'edit', 'react', 'pin', 'meta', 'system')),
    epoch INTEGER NOT NULL,
    target_seq INTEGER,
    ts INTEGER NOT NULL,
    expires_at INTEGER,
    local_only INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL CHECK (content IN ('open', 'sealed', 'system', 'deleted')),
    nonce BLOB,
    body BLOB
);

CREATE UNIQUE INDEX messages_seq ON messages (conversation_id, seq);
CREATE INDEX messages_client ON messages (conversation_id, client_id);
CREATE INDEX messages_target ON messages (conversation_id, target_seq) WHERE target_seq IS NOT NULL;
CREATE INDEX messages_expiring ON messages (expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX messages_sealed ON messages (conversation_id, epoch) WHERE content = 'sealed';

CREATE VIRTUAL TABLE search USING fts5(tokens, content = '', contentless_delete = 1, tokenize = 'ascii');

CREATE TABLE message_embeds (
    message_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    conversation_id BLOB NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('track', 'playlist', 'user', 'room_invite')),
    nonce BLOB NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (message_id, position)
) WITHOUT ROWID;

CREATE INDEX message_embeds_conversation ON message_embeds (conversation_id, kind);

CREATE TABLE conversation_keys (
    conversation_id BLOB NOT NULL,
    epoch INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    ck BLOB NOT NULL,
    PRIMARY KEY (conversation_id, epoch)
) WITHOUT ROWID;

CREATE TABLE drafts (
    conversation_id BLOB PRIMARY KEY,
    updated_at INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    body BLOB NOT NULL
) WITHOUT ROWID;

CREATE TABLE outbox (
    client_id BLOB PRIMARY KEY,
    conversation_id BLOB NOT NULL,
    class TEXT NOT NULL CHECK (class IN ('post', 'edit', 'react', 'pin', 'meta')),
    target_seq INTEGER,
    created_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_at INTEGER NOT NULL,
    failed INTEGER NOT NULL DEFAULT 0,
    nonce BLOB NOT NULL,
    body BLOB NOT NULL
) WITHOUT ROWID;

CREATE INDEX outbox_due ON outbox (next_at) WHERE failed = 0;
CREATE INDEX outbox_conversation ON outbox (conversation_id, created_at);
";

const V2: &str = "
CREATE TABLE hidden_messages (
    conversation_id BLOB NOT NULL,
    seq INTEGER NOT NULL,
    PRIMARY KEY (conversation_id, seq)
) WITHOUT ROWID;
";

const STEPS: [&str; 2] = [V1, V2];

pub fn prepare(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "secure_delete", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")
}

pub fn version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let from = version(conn)?;
    if from >= VERSION {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for step in STEPS.iter().skip(usize::try_from(from).unwrap_or_default()) {
        tx.execute_batch(step)?;
    }
    tx.pragma_update(None, "user_version", VERSION)?;
    tx.commit()
}

#[cfg(test)]
#[path = "migrations_tests.rs"]
mod tests;
