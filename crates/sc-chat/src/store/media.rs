use rusqlite::{Row, params};
use sc_chat_wire::prost::Message;
use sc_chat_wire::proto::Attachment;
use uuid::Uuid;

use super::{Store, StoreKeys, seal};
use crate::contracts::{ChatError, ChatResult, Millis};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    Queued,
    Reserved,
    Uploading,
    Uploaded,
    Committed,
    Failed,
}

impl UploadState {
    fn parse(text: &str) -> ChatResult<Self> {
        Ok(match text {
            "queued" => Self::Queued,
            "reserved" => Self::Reserved,
            "uploading" => Self::Uploading,
            "uploaded" => Self::Uploaded,
            "committed" => Self::Committed,
            "failed" => Self::Failed,
            _ => return Err(ChatError::Store),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadObject {
    Image,
    Thumb,
}

impl UploadObject {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Thumb => "thumb",
        }
    }

    fn parse(text: &str) -> ChatResult<Self> {
        match text {
            "image" => Ok(Self::Image),
            "thumb" => Ok(Self::Thumb),
            _ => Err(ChatError::Store),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaUpload {
    pub id: Uuid,
    pub album: Uuid,
    pub position: u32,
    pub image_ref: String,
    pub thumb_ref: String,
    pub attachment: Attachment,
    pub state: UploadState,
    pub object: UploadObject,
    pub reserve_id: Option<String>,
    pub stage_id: Option<String>,
    pub parts_sent: u32,
    pub presign_expires_at: Option<Millis>,
    pub attempts: u32,
    pub created_at: Millis,
    pub updated_at: Millis,
}

impl MediaUpload {
    pub fn presign_expired(&self, now: Millis) -> bool {
        self.presign_expires_at.is_none_or(|at| at <= now)
    }
}

#[derive(Debug, Clone)]
pub struct NewUpload {
    pub album: Uuid,
    pub position: u32,
    pub image_ref: String,
    pub thumb_ref: String,
    pub attachment: Attachment,
}

#[derive(Debug, Clone)]
pub struct Reserved {
    pub reserve_id: String,
    pub media_id: String,
    pub thumb_media_id: String,
    pub presign_expires_at: Millis,
}

const COLUMNS: &str = "id, album, position, image_ref, thumb_ref, state, object, reserve_id, stage_id, parts_sent, presign_expires_at, attempts, created_at, updated_at, nonce, attachment";

impl Store {
    pub fn enqueue_upload(&self, upload: &NewUpload, now: Millis) -> ChatResult<Uuid> {
        let id = Uuid::new_v4();
        let sealed = seal::seal(
            &self.keys.body,
            &aad(id),
            &upload.attachment.encode_to_vec(),
        )?;
        self.conn()?.execute(
            "INSERT INTO media_uploads (id, album, position, image_ref, thumb_ref, created_at, updated_at, nonce, attachment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8)",
            params![
                id.as_bytes(),
                upload.album.as_bytes(),
                upload.position,
                upload.image_ref,
                upload.thumb_ref,
                now,
                sealed.nonce,
                sealed.body,
            ],
        )?;
        Ok(id)
    }

    pub fn upload(&self, id: Uuid) -> ChatResult<Option<MediaUpload>> {
        let conn = self.conn()?;
        load(&conn, &self.keys, "id = ?1", [id.as_bytes()]).map(|mut found| found.pop())
    }

    pub fn album_uploads(&self, album: Uuid) -> ChatResult<Vec<MediaUpload>> {
        let conn = self.conn()?;
        load(
            &conn,
            &self.keys,
            "album = ?1 ORDER BY position",
            [album.as_bytes()],
        )
    }

    pub fn next_upload(&self) -> ChatResult<Option<MediaUpload>> {
        let conn = self.conn()?;
        load(
            &conn,
            &self.keys,
            "state NOT IN ('committed', 'failed') ORDER BY created_at, position LIMIT 1",
            [],
        )
        .map(|mut found| found.pop())
    }

    pub fn upload_reserved(&self, id: Uuid, reserved: &Reserved, now: Millis) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let mut found = load(&tx, &self.keys, "id = ?1", [id.as_bytes()])?;
        let mut upload = found.pop().ok_or(ChatError::NotFound)?;
        upload.attachment.media_id = reserved.media_id.clone();
        if let Some(thumb) = upload.attachment.thumb.as_mut() {
            thumb.media_id = reserved.thumb_media_id.clone();
        }
        let sealed = seal::seal(
            &self.keys.body,
            &aad(id),
            &upload.attachment.encode_to_vec(),
        )?;
        tx.execute(
            "UPDATE media_uploads SET state = 'reserved', object = 'image', reserve_id = ?2, stage_id = NULL, parts_sent = 0, presign_expires_at = ?3, updated_at = ?4, nonce = ?5, attachment = ?6 WHERE id = ?1",
            params![
                id.as_bytes(),
                reserved.reserve_id,
                reserved.presign_expires_at,
                now,
                sealed.nonce,
                sealed.body,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn upload_progress(
        &self,
        id: Uuid,
        object: UploadObject,
        parts_sent: u32,
        stage_id: Option<&str>,
        now: Millis,
    ) -> ChatResult<()> {
        self.touch(
            "UPDATE media_uploads SET state = 'uploading', object = ?2, parts_sent = ?3, stage_id = ?4, updated_at = ?5 WHERE id = ?1 AND state IN ('reserved', 'uploading')",
            params![id.as_bytes(), object.as_str(), parts_sent, stage_id, now],
        )
    }

    pub fn upload_object_done(
        &self,
        id: Uuid,
        object: UploadObject,
        now: Millis,
    ) -> ChatResult<()> {
        match object {
            UploadObject::Image => self.touch(
                "UPDATE media_uploads SET state = 'uploading', object = 'thumb', parts_sent = 0, stage_id = NULL, updated_at = ?2 WHERE id = ?1 AND state IN ('reserved', 'uploading')",
                params![id.as_bytes(), now],
            ),
            UploadObject::Thumb => self.touch(
                "UPDATE media_uploads SET state = 'uploaded', parts_sent = 0, stage_id = NULL, updated_at = ?2 WHERE id = ?1 AND state IN ('reserved', 'uploading')",
                params![id.as_bytes(), now],
            ),
        }
    }

    pub fn upload_committed(&self, id: Uuid, now: Millis) -> ChatResult<()> {
        self.touch(
            "UPDATE media_uploads SET state = 'committed', updated_at = ?2 WHERE id = ?1 AND state = 'uploaded'",
            params![id.as_bytes(), now],
        )
    }

    pub fn upload_failed(&self, id: Uuid, retryable: bool, now: Millis) -> ChatResult<()> {
        self.touch(
            "UPDATE media_uploads SET attempts = attempts + 1, state = CASE WHEN ?2 THEN state ELSE 'failed' END, updated_at = ?3 WHERE id = ?1 AND state NOT IN ('committed', 'failed')",
            params![id.as_bytes(), retryable, now],
        )
    }

    pub fn upload_restart(&self, id: Uuid, now: Millis) -> ChatResult<()> {
        self.touch(
            "UPDATE media_uploads SET state = 'queued', object = 'image', reserve_id = NULL, stage_id = NULL, parts_sent = 0, presign_expires_at = NULL, updated_at = ?2 WHERE id = ?1 AND state <> 'committed'",
            params![id.as_bytes(), now],
        )
    }

    pub fn remove_album_uploads(&self, album: Uuid) -> ChatResult<Vec<(String, String)>> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let refs = load(&tx, &self.keys, "album = ?1", [album.as_bytes()])?
            .into_iter()
            .map(|upload| (upload.image_ref, upload.thumb_ref))
            .collect();
        tx.execute(
            "DELETE FROM media_uploads WHERE album = ?1",
            [album.as_bytes()],
        )?;
        tx.commit()?;
        Ok(refs)
    }

    fn touch(&self, sql: &str, params: impl rusqlite::Params) -> ChatResult<()> {
        self.conn()?.execute(sql, params)?;
        Ok(())
    }
}

fn aad(id: Uuid) -> Vec<u8> {
    seal::tagged_aad("media-upload", &[id.as_bytes()])
}

fn load(
    conn: &rusqlite::Connection,
    keys: &StoreKeys,
    filter: &str,
    params: impl rusqlite::Params,
) -> ChatResult<Vec<MediaUpload>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM media_uploads WHERE {filter}"
    ))?;
    let rows = statement
        .query_map(params, raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(|row| row.open(keys)).collect()
}

struct Raw {
    upload: MediaUpload,
    nonce: Vec<u8>,
    sealed: Vec<u8>,
    state: String,
    object: String,
}

impl Raw {
    fn open(mut self, keys: &StoreKeys) -> ChatResult<MediaUpload> {
        let plain = seal::open(&keys.body, &aad(self.upload.id), &self.nonce, &self.sealed)?;
        self.upload.attachment =
            Attachment::decode(plain.as_slice()).map_err(|_| ChatError::Store)?;
        self.upload.state = UploadState::parse(&self.state)?;
        self.upload.object = UploadObject::parse(&self.object)?;
        Ok(self.upload)
    }
}

fn raw(row: &Row) -> rusqlite::Result<Raw> {
    let uuid = |at: usize| -> rusqlite::Result<Uuid> {
        Uuid::from_slice(&row.get::<_, Vec<u8>>(at)?).map_err(|_| rusqlite::Error::InvalidQuery)
    };
    Ok(Raw {
        upload: MediaUpload {
            id: uuid(0)?,
            album: uuid(1)?,
            position: row.get(2)?,
            image_ref: row.get(3)?,
            thumb_ref: row.get(4)?,
            attachment: Attachment::default(),
            state: UploadState::Queued,
            object: UploadObject::Image,
            reserve_id: row.get(7)?,
            stage_id: row.get(8)?,
            parts_sent: row.get(9)?,
            presign_expires_at: row.get(10)?,
            attempts: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        },
        state: row.get(5)?,
        object: row.get(6)?,
        nonce: row.get(14)?,
        sealed: row.get(15)?,
    })
}
