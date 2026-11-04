use std::sync::Arc;

use uuid::Uuid;

use super::Store;
use super::media_rows::MediaUpload;
use super::media_rows::load;
use crate::contracts::{CoreEvent, Events, MediaUploadView, MediaUploadsEvent};

impl Store {
    pub fn watch_uploads(&self, events: Option<Arc<dyn Events>>) {
        if let Ok(mut watch) = self.uploads_watch.write() {
            *watch = events;
        }
    }

    pub(super) fn uploads_changed(&self, ids: &[Uuid]) {
        let Some(events) = self.watcher() else {
            return;
        };
        let uploads: Vec<MediaUploadView> = ids
            .iter()
            .filter_map(|id| self.changed_row(*id))
            .map(|upload| MediaUploadView::from(&upload))
            .collect();
        if !uploads.is_empty() {
            events.emit(CoreEvent::MediaUploads(MediaUploadsEvent {
                uploads,
                removed: Vec::new(),
            }));
        }
    }

    pub(super) fn uploads_removed(&self, removed: Vec<Uuid>) {
        if let Some(events) = self.watcher().filter(|_| !removed.is_empty()) {
            events.emit(CoreEvent::MediaUploads(MediaUploadsEvent {
                uploads: Vec::new(),
                removed,
            }));
        }
    }

    fn watcher(&self) -> Option<Arc<dyn Events>> {
        self.uploads_watch.read().ok()?.clone()
    }

    fn changed_row(&self, id: Uuid) -> Option<MediaUpload> {
        let conn = self.conn().ok()?;
        load(&conn, &self.keys, "id = ?1", [id.as_bytes()])
            .ok()?
            .pop()
    }
}

impl From<&MediaUpload> for MediaUploadView {
    fn from(upload: &MediaUpload) -> Self {
        Self {
            id: upload.id,
            album: upload.album,
            position: upload.position,
            conversation_id: upload.conversation_id,
            kind: upload.kind,
            state: upload.state,
            object: upload.object,
            parts_sent: upload.parts_sent,
            image_bytes: upload.attachment.size,
            thumb_bytes: upload
                .attachment
                .thumb
                .as_ref()
                .map_or(0, |thumb| thumb.size),
            attempts: upload.attempts,
            updated_at: upload.updated_at,
        }
    }
}
