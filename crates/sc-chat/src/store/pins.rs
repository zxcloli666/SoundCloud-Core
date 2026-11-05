use super::{Store, StoredContent, content, items, project};
use crate::contracts::{ChatResult, ConvId, Seq};
use crate::envelope::Content;

impl Store {
    pub fn pins(&self, id: ConvId) -> ChatResult<Vec<Seq>> {
        let conn = self.conn()?;
        let rows = items::load(
            &conn,
            &self.keys,
            "conversation_id = ?1 AND class = 'pin' ORDER BY seq",
            [id.as_bytes()],
        )?;
        let mut pinned: Vec<Seq> = Vec::new();
        for row in rows {
            let StoredContent::Open(bytes) = &row.message.content else {
                continue;
            };
            let Content::Pin { target, on } = content::decode(bytes) else {
                continue;
            };
            let Some(target) = row
                .message
                .target_seq
                .filter(|seq| project::aims_at(&row.message, target, Some(*seq)))
            else {
                continue;
            };
            pinned.retain(|seq| *seq != target);
            if on {
                pinned.push(target);
            }
        }
        Ok(pinned)
    }
}
