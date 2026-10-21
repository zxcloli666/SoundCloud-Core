use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sc_chat::crypto::DeviceKeys;
use sc_chat::sc_chat_wire::prost::Message as _;
use uuid::Uuid;

use super::Stack;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct Person {
    pub user_id: String,
    pub name: String,
    pub connection_id: Uuid,
    pub session_id: Uuid,
    pub device_id: Uuid,
    pub keys: Arc<DeviceKeys>,
}

impl Stack {
    pub fn person(&self, name: &str) -> Person {
        let user_id = next_user_id();
        let connection_id = Uuid::new_v4();
        self.sql(&format!(
            "INSERT INTO soundcloud_connections (id, soundcloud_user_id, username, access_token, refresh_token, expires_at, scope) VALUES ('{connection_id}', '{user_id}', '{name}', 'it', 'it', now() + interval '30 days', 'non-expiring')"
        ));
        self.device_of(&Person {
            user_id,
            name: name.to_owned(),
            connection_id,
            session_id: Uuid::nil(),
            device_id: Uuid::nil(),
            keys: Arc::new(DeviceKeys::generate().unwrap()),
        })
    }

    pub fn device_of(&self, person: &Person) -> Person {
        let keys = Arc::new(DeviceKeys::generate().unwrap());
        let device_id = Uuid::new_v4();
        let bind = keys.bind(device_id, now_ms()).unwrap().encode_to_vec();
        let session_id = self.session(person, Some(&bind));
        Person {
            session_id,
            device_id,
            keys,
            ..person.clone()
        }
    }

    pub fn unbound_session(&self, person: &Person) -> Uuid {
        self.session(person, None)
    }

    fn session(&self, person: &Person, bind: Option<&[u8]>) -> Uuid {
        let session_id = Uuid::new_v4();
        let bind = match bind {
            Some(bind) => format!("decode('{}', 'hex')", hex(bind)),
            None => "NULL".to_owned(),
        };
        self.sql(&format!(
            "INSERT INTO sessions (id, soundcloud_connection_id, chat_bind) VALUES ('{session_id}', '{}', {bind})",
            person.connection_id
        ));
        session_id
    }

    pub fn follow(&self, follower: &Person, followed: &Person) {
        self.sql(&format!(
            "INSERT INTO user_followings (user_id, target_user_urn) VALUES ('{}', 'soundcloud:users:{}') ON CONFLICT DO NOTHING",
            follower.user_id, followed.user_id
        ));
    }

    pub fn oauth_app(&self) {
        self.sql(
            "INSERT INTO oauth_apps (id, name, client_id, client_secret, redirect_uri) VALUES (gen_random_uuid(), 'chat-it', 'chat-it', 'chat-it', 'http://127.0.0.1/auth/callback') ON CONFLICT DO NOTHING",
        );
    }

    pub fn age_device(&self, person: &Person, hours: u32) {
        self.sql(&format!(
            "UPDATE chat.devices SET created_at = created_at - interval '{hours} hours' WHERE id = '{}'",
            person.device_id
        ));
    }

    pub fn age_session(&self, person: &Person, hours: u32) {
        self.sql(&format!(
            "UPDATE sessions SET created_at = created_at - interval '{hours} hours' WHERE id = '{}'",
            person.session_id
        ));
    }
}

fn next_user_id() -> String {
    let run = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    format!("{}", run * 100 + NEXT.fetch_add(1, Ordering::Relaxed))
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
