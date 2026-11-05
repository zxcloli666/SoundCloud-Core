use rusqlite::params;

use super::history::{page_limit, project_rows};
use super::items;
use super::project::Context;
use super::{Store, meta, people};
use crate::contracts::{ChatResult, ConvId, HistoryPage, Seq};

const IMAGE_POSTS: &str = "conversation_id = ?1 AND media > 0 AND class = 'post' AND local_only = 0 AND seq > 0 AND seq < ?2 ORDER BY seq DESC LIMIT ?3";

impl Store {
    pub fn images_before(
        &self,
        id: ConvId,
        before: Option<Seq>,
        limit: u32,
    ) -> ChatResult<HistoryPage> {
        let conn = self.conn()?;
        let me = meta::me(&conn)?;
        let marks = people::marks(&conn, id)?;
        let context = Context {
            me: me.as_deref(),
            marks: &marks,
        };
        let limit = page_limit(limit);
        let mut rows = items::load(
            &conn,
            &self.keys,
            IMAGE_POSTS,
            params![id.as_bytes(), before.unwrap_or(Seq::MAX), limit as i64 + 1],
        )?;
        let more = rows.len() > limit;
        rows.truncate(limit);
        let messages = project_rows(&conn, &self.keys, id, &rows, context)?;
        Ok(HistoryPage { messages, more })
    }
}

#[cfg(test)]
#[path = "gallery_tests.rs"]
mod tests;
