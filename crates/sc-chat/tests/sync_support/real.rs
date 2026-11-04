use std::sync::Arc;

use sc_chat::api::wire::b64;
use sc_chat::api::{Api, Identity};
use sc_chat::channel::Channel;
use sc_chat::crypto::DeviceKeys;
use sc_chat::store::{Store, SyncState};
use sc_chat::sync::{PollPlan, SyncEngine};
use sc_chat::{ConvId, ConversationSummary, Tier};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{DEVICE, ME, Recorder, START_MS, Server, TestClock};

pub const INC: Uuid = Uuid::from_u128(0x1c);
pub const CK: [u8; 32] = [7; 32];

pub struct Real {
    pub server: Arc<Server>,
    pub events: Arc<Recorder>,
    pub keys: Arc<DeviceKeys>,
    pub engine: SyncEngine,
}

pub fn real(conversations: &[ConversationSummary]) -> Real {
    let clock = Arc::new(TestClock::at(START_MS));
    let server = Arc::new(Server::new(&[Tier::Direct], clock.clone()));
    let api = Arc::new(Api::with_exchange(
        Arc::new(Channel::new(clock)),
        server.clone(),
        server.clone(),
    ));
    let keys = Arc::new(DeviceKeys::generate().unwrap());
    api.set_identity(Some(Identity {
        session_id: "session".into(),
        device_id: DEVICE,
        keys: keys.clone(),
    }));
    let store = Store::open_in_memory(&[9; 32]).unwrap();
    store.set_me(ME).unwrap();
    store
        .set_sync_state(SyncState {
            pos: 1,
            inc: Some(INC),
        })
        .unwrap();
    store.put_conversations(conversations).unwrap();
    let events = Arc::new(Recorder::default());
    let engine = SyncEngine::new(api, Arc::new(store), events.clone());
    engine.set_user(Some(ME.into()));
    Real {
        server,
        events,
        keys,
        engine,
    }
}

impl Real {
    pub fn sealed_keys(&self, conversation_id: ConvId, epochs: &[u32]) -> Value {
        let sealed: Vec<Value> = epochs
            .iter()
            .map(|epoch| {
                let sealed = sc_chat::sc_chat_wire::ck::seal_for_device(
                    &self.keys.box_public().unwrap(),
                    conversation_id.as_bytes(),
                    *epoch,
                    DEVICE.as_bytes(),
                    &CK,
                    [5; 32],
                )
                .unwrap();
                json!({"epoch": epoch, "sealed": b64::encode(&sealed)})
            })
            .collect();
        Value::Array(sealed)
    }

    pub fn serve_keys(&self, conversation_id: ConvId, epochs: &[u32]) {
        self.server.json(
            "GET",
            &format!("/chat/conversations/{conversation_id}/keys"),
            self.sealed_keys(conversation_id, epochs),
        );
    }
}

pub fn direct_plan() -> PollPlan {
    PollPlan {
        tier: Tier::Direct,
        hold_ms: 0,
        delay_ms: 0,
    }
}
