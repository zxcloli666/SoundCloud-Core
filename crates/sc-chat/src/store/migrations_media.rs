pub const V3: &str = "
CREATE TABLE media_uploads (
    id BLOB PRIMARY KEY,
    album BLOB NOT NULL,
    position INTEGER NOT NULL,
    conversation_id BLOB NOT NULL,
    image_ref TEXT NOT NULL,
    thumb_ref TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'reserved', 'uploading', 'uploaded', 'committed', 'failed')),
    object TEXT NOT NULL DEFAULT 'image' CHECK (object IN ('image', 'thumb')),
    media_id BLOB,
    stage_id TEXT,
    parts_sent INTEGER NOT NULL DEFAULT 0,
    presign_expires_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    attachment BLOB NOT NULL
) WITHOUT ROWID;

CREATE INDEX media_uploads_album ON media_uploads (album, position);
CREATE INDEX media_uploads_open ON media_uploads (created_at, position) WHERE state NOT IN ('committed', 'failed');
";

pub const V4: &str = "
CREATE TABLE message_media (
    message_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    conversation_id BLOB NOT NULL,
    media_id BLOB NOT NULL,
    nonce BLOB NOT NULL,
    body BLOB NOT NULL,
    PRIMARY KEY (message_id, position)
) WITHOUT ROWID;

CREATE INDEX message_media_lookup ON message_media (conversation_id, media_id);

CREATE TABLE group_meta (
    conversation_id BLOB PRIMARY KEY,
    avatar_id BLOB,
    nonce BLOB NOT NULL,
    body BLOB NOT NULL
) WITHOUT ROWID;

CREATE TABLE media_uploads_v4 (
    id BLOB PRIMARY KEY,
    album BLOB NOT NULL,
    position INTEGER NOT NULL,
    conversation_id BLOB NOT NULL,
    kind TEXT NOT NULL DEFAULT 'image' CHECK (kind IN ('image', 'avatar')),
    image_ref TEXT NOT NULL,
    thumb_ref TEXT,
    state TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'reserved', 'uploading', 'uploaded', 'committed', 'failed')),
    object TEXT NOT NULL DEFAULT 'image' CHECK (object IN ('image', 'thumb')),
    media_id BLOB,
    stage_id TEXT,
    parts_sent INTEGER NOT NULL DEFAULT 0,
    presign_expires_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    attachment BLOB NOT NULL
) WITHOUT ROWID;

INSERT INTO media_uploads_v4 (id, album, position, conversation_id, image_ref, thumb_ref, state, object, media_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment)
SELECT id, album, position, conversation_id, image_ref, thumb_ref, state, object, media_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment FROM media_uploads;

DROP TABLE media_uploads;
ALTER TABLE media_uploads_v4 RENAME TO media_uploads;

CREATE INDEX media_uploads_album ON media_uploads (album, position);
CREATE INDEX media_uploads_open ON media_uploads (created_at, position) WHERE state NOT IN ('committed', 'failed');
";

pub const V5: &str = "
CREATE TABLE media_uploads_v5 (
    id BLOB PRIMARY KEY,
    album BLOB NOT NULL,
    position INTEGER NOT NULL,
    conversation_id BLOB NOT NULL,
    kind TEXT NOT NULL DEFAULT 'image' CHECK (kind IN ('image', 'avatar', 'backdrop')),
    image_ref TEXT NOT NULL,
    thumb_ref TEXT,
    state TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'reserved', 'uploading', 'uploaded', 'committed', 'failed')),
    object TEXT NOT NULL DEFAULT 'image' CHECK (object IN ('image', 'thumb')),
    media_id BLOB,
    stage_id TEXT,
    parts_sent INTEGER NOT NULL DEFAULT 0,
    presign_expires_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    nonce BLOB NOT NULL,
    attachment BLOB NOT NULL
) WITHOUT ROWID;

INSERT INTO media_uploads_v5 (id, album, position, conversation_id, kind, image_ref, thumb_ref, state, object, media_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment)
SELECT id, album, position, conversation_id, kind, image_ref, thumb_ref, state, object, media_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment FROM media_uploads;

DROP TABLE media_uploads;
ALTER TABLE media_uploads_v5 RENAME TO media_uploads;

CREATE INDEX media_uploads_album ON media_uploads (album, position);
CREATE INDEX media_uploads_open ON media_uploads (created_at, position) WHERE state NOT IN ('committed', 'failed');
";

pub const V6: &str = "
ALTER TABLE messages ADD COLUMN media INTEGER NOT NULL DEFAULT 0;

UPDATE messages SET media = (SELECT count(*) FROM message_media WHERE message_media.message_id = messages.id)
WHERE id IN (SELECT message_id FROM message_media);

CREATE INDEX messages_gallery ON messages (conversation_id, seq) WHERE media > 0;
";
