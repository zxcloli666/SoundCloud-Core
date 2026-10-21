use rusqlite::Row;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use crate::contracts::{ChatError, ChatResult, MessageClass};

pub fn label<T: Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(text)) => text,
        _ => String::new(),
    }
}

pub struct Label<T>(pub T);

impl<T: DeserializeOwned> FromSql for Label<T> {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        serde_json::from_value(serde_json::Value::String(text.to_owned()))
            .map(Label)
            .map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

pub fn class_at(row: &Row<'_>, index: usize) -> rusqlite::Result<MessageClass> {
    let raw: String = row.get(index)?;
    MessageClass::parse(&raw).ok_or(rusqlite::Error::InvalidColumnType(
        index,
        raw,
        rusqlite::types::Type::Text,
    ))
}

pub fn uuid_at(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    row.get::<_, [u8; 16]>(index).map(Uuid::from_bytes)
}

pub fn uuid_opt_at(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Uuid>> {
    Ok(row.get::<_, Option<[u8; 16]>>(index)?.map(Uuid::from_bytes))
}

pub fn to_json<T: Serialize>(value: &T) -> ChatResult<Vec<u8>> {
    serde_json::to_vec(value).map_err(|_| ChatError::Store)
}

pub fn from_json<T: DeserializeOwned>(raw: &[u8]) -> ChatResult<T> {
    serde_json::from_slice(raw).map_err(|_| ChatError::Store)
}
