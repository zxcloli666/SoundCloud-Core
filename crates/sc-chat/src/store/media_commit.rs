use std::collections::HashSet;
use std::sync::MutexGuard;

use rusqlite::{Connection, params};
use uuid::Uuid;

use super::Store;
use crate::contracts::{ChatError, ChatResult, Millis};

impl Store {
    pub fn upload_committed(&self, id: Uuid, now: Millis) -> ChatResult<()> {
        let committed = {
            let mut conn = self.conn()?;
            let tx = conn.transaction()?;
            let changed = tx.execute(
                "UPDATE media_uploads SET state = 'committed', updated_at = ?2 WHERE id = ?1 AND state = 'uploaded'",
                params![id.as_bytes(), now],
            )?;
            if changed == 0 && !kept(&tx, id)? {
                return Err(ChatError::NotFound);
            }
            tx.commit()?;
            if changed > 0 {
                self.committed()?.insert(id);
            }
            changed > 0
        };
        if committed {
            self.uploads_changed(&[id]);
        }
        Ok(())
    }

    pub fn was_committed(&self, id: Uuid) -> ChatResult<bool> {
        Ok(self.committed()?.contains(&id))
    }

    fn committed(&self) -> ChatResult<MutexGuard<'_, HashSet<Uuid>>> {
        self.committed.lock().map_err(|_| ChatError::Store)
    }
}

fn kept(conn: &Connection, id: Uuid) -> ChatResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM media_uploads WHERE id = ?1)",
        [id.as_bytes()],
        |row| row.get(0),
    )?)
}
