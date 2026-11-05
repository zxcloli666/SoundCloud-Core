use rusqlite::params;
use sc_chat_wire::prost::Message;
use uuid::Uuid;

use super::codec::label;
use super::media_rows::{MediaUpload, NewUpload, Reserved, aad, load};
use super::{Store, seal};
use crate::contracts::{ChatError, ChatResult, MediaKind, Millis, UploadObject};

impl Store {
    pub fn enqueue_upload(&self, upload: &NewUpload, now: Millis) -> ChatResult<Uuid> {
        let image = upload.kind == MediaKind::Image;
        if image != upload.thumb_ref.is_some() || image != upload.attachment.thumb.is_some() {
            return Err(ChatError::Invalid);
        }
        let id = Uuid::new_v4();
        let sealed = seal::seal(
            &self.keys.body,
            &aad(id),
            &upload.attachment.encode_to_vec(),
        )?;
        self.conn()?.execute(
            "INSERT INTO media_uploads (id, album, position, conversation_id, kind, image_ref, thumb_ref, created_at, updated_at, nonce, attachment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10)",
            params![
                id.as_bytes(),
                upload.album.as_bytes(),
                upload.position,
                upload.conversation_id.as_bytes(),
                label(upload.kind),
                upload.image_ref,
                upload.thumb_ref,
                now,
                sealed.nonce,
                sealed.body,
            ],
        )?;
        self.uploads_changed(&[id]);
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

    pub fn uploads(&self) -> ChatResult<Vec<MediaUpload>> {
        let conn = self.conn()?;
        load(&conn, &self.keys, "1 ORDER BY created_at, position", [])
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
        self.reserve_row(id, reserved, now)?;
        self.uploads_changed(&[id]);
        Ok(())
    }

    fn reserve_row(&self, id: Uuid, reserved: &Reserved, now: Millis) -> ChatResult<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let mut found = load(&tx, &self.keys, "id = ?1", [id.as_bytes()])?;
        let mut upload = found.pop().ok_or(ChatError::NotFound)?;
        if same_reservation(&upload, reserved) {
            tx.execute(
                "UPDATE media_uploads SET state = CASE WHEN state = 'queued' THEN 'reserved' ELSE state END, presign_expires_at = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.as_bytes(), reserved.presign_expires_at, now],
            )?;
            tx.commit()?;
            return Ok(());
        }
        upload.attachment.media_id = reserved.media_id.to_string();
        if let (Some(thumb), Some(thumb_id)) =
            (upload.attachment.thumb.as_mut(), reserved.thumb_media_id)
        {
            thumb.media_id = thumb_id.to_string();
        }
        let sealed = seal::seal(
            &self.keys.body,
            &aad(id),
            &upload.attachment.encode_to_vec(),
        )?;
        tx.execute(
            "UPDATE media_uploads SET state = 'reserved', object = 'image', media_id = ?2, stage_id = NULL, parts_sent = 0, presign_expires_at = ?3, updated_at = ?4, nonce = ?5, attachment = ?6 WHERE id = ?1",
            params![
                id.as_bytes(),
                reserved.media_id.as_bytes(),
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
            id,
            "UPDATE media_uploads SET state = 'uploading', object = ?2, parts_sent = ?3, stage_id = ?4, updated_at = ?5 WHERE id = ?1 AND state IN ('reserved', 'uploading')",
            params![id.as_bytes(), label(object), parts_sent, stage_id, now],
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
                id,
                "UPDATE media_uploads SET state = CASE kind WHEN 'image' THEN 'uploading' ELSE 'uploaded' END, object = CASE kind WHEN 'image' THEN 'thumb' ELSE 'image' END, parts_sent = 0, stage_id = NULL, updated_at = ?2 WHERE id = ?1 AND state IN ('reserved', 'uploading')",
                params![id.as_bytes(), now],
            ),
            UploadObject::Thumb => self.touch(
                id,
                "UPDATE media_uploads SET state = 'uploaded', parts_sent = 0, stage_id = NULL, updated_at = ?2 WHERE id = ?1 AND kind = 'image' AND state IN ('reserved', 'uploading')",
                params![id.as_bytes(), now],
            ),
        }
    }

    pub fn upload_redo(&self, id: Uuid, object: UploadObject, now: Millis) -> ChatResult<()> {
        self.touch(
            id,
            "UPDATE media_uploads SET state = 'uploading', object = ?2, parts_sent = 0, stage_id = NULL, updated_at = ?3 WHERE id = ?1 AND (?2 = 'image' OR kind = 'image') AND state IN ('reserved', 'uploading', 'uploaded')",
            params![id.as_bytes(), label(object), now],
        )
    }

    pub fn upload_failed(&self, id: Uuid, retryable: bool, now: Millis) -> ChatResult<()> {
        self.touch(
            id,
            "UPDATE media_uploads SET attempts = attempts + 1, state = CASE WHEN ?2 THEN state ELSE 'failed' END, updated_at = ?3 WHERE id = ?1 AND state NOT IN ('committed', 'failed')",
            params![id.as_bytes(), retryable, now],
        )
    }

    pub fn upload_restart(&self, id: Uuid, now: Millis) -> ChatResult<()> {
        self.touch(
            id,
            "UPDATE media_uploads SET state = 'queued', object = 'image', media_id = NULL, stage_id = NULL, parts_sent = 0, presign_expires_at = NULL, updated_at = ?2 WHERE id = ?1 AND state <> 'committed'",
            params![id.as_bytes(), now],
        )
    }

    pub fn remove_album_uploads(&self, album: Uuid) -> ChatResult<Vec<MediaUpload>> {
        let removed = {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            let removed = load(&tx, &self.keys, "album = ?1", [album.as_bytes()])?;
            tx.execute(
                "DELETE FROM media_uploads WHERE album = ?1",
                [album.as_bytes()],
            )?;
            tx.commit()?;
            removed
        };
        self.uploads_removed(removed.iter().map(|upload| upload.id).collect());
        Ok(removed)
    }

    pub fn remove_upload(&self, id: Uuid) -> ChatResult<Option<MediaUpload>> {
        let removed = {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            let removed = load(&tx, &self.keys, "id = ?1", [id.as_bytes()])?.pop();
            tx.execute("DELETE FROM media_uploads WHERE id = ?1", [id.as_bytes()])?;
            tx.commit()?;
            removed
        };
        if removed.is_some() {
            self.uploads_removed(vec![id]);
        }
        Ok(removed)
    }

    fn touch(&self, id: Uuid, sql: &str, params: impl rusqlite::Params) -> ChatResult<()> {
        if self.conn()?.execute(sql, params)? > 0 {
            self.uploads_changed(&[id]);
        }
        Ok(())
    }
}

fn same_reservation(upload: &MediaUpload, reserved: &Reserved) -> bool {
    let thumb_id = upload
        .attachment
        .thumb
        .as_ref()
        .map(|thumb| thumb.media_id.clone());
    upload.media_id == Some(reserved.media_id)
        && thumb_id == reserved.thumb_media_id.map(|id| id.to_string())
}
