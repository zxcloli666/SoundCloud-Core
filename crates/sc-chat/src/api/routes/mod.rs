mod auth;
mod conversations;
mod devices;
mod members;
mod messages;
mod rooms;
mod safety;
mod sync;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use sync::SyncReply;

use super::{Api, CallOptions};
use crate::contracts::ChatResult;

const NO_BODY: Option<&()> = None;

impl Api {
    async fn get<T: DeserializeOwned>(&self, path: &str, query: &str) -> ChatResult<T> {
        self.json("GET", path, query, NO_BODY, CallOptions::default())
            .await
    }

    async fn post<T: DeserializeOwned, B: Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
    ) -> ChatResult<T> {
        self.json("POST", path, "", Some(body), CallOptions::default())
            .await
    }

    async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> ChatResult<T> {
        self.json("POST", path, "", NO_BODY, CallOptions::default())
            .await
    }

    async fn put<T: DeserializeOwned, B: Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
    ) -> ChatResult<T> {
        self.json("PUT", path, "", Some(body), CallOptions::default())
            .await
    }

    async fn put_empty<T: DeserializeOwned>(&self, path: &str) -> ChatResult<T> {
        self.json("PUT", path, "", NO_BODY, CallOptions::default())
            .await
    }

    async fn patch<T: DeserializeOwned, B: Serialize + Sync>(
        &self,
        path: &str,
        body: &B,
    ) -> ChatResult<T> {
        self.json("PATCH", path, "", Some(body), CallOptions::default())
            .await
    }

    async fn delete(&self, path: &str, query: &str) -> ChatResult<()> {
        self.json("DELETE", path, query, NO_BODY, CallOptions::default())
            .await
    }
}
