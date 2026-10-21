#![allow(dead_code, unused_imports)]

mod cipher;
mod clock;
mod events;
pub mod fixtures;
mod local;
mod server;

use std::sync::Arc;

use sc_chat::Tier;
use sc_chat::api::{Api, Identity};
use sc_chat::channel::Channel;
use sc_chat::crypto::DeviceKeys;
use sc_chat::sync::SyncEngine;
use uuid::Uuid;

pub use cipher::{TestCipher, ck_bytes};
pub use clock::TestClock;
pub use events::Recorder;
pub use local::MemoryLocal;
pub use server::{Call, Server, Step};

pub const ME: &str = "100";
pub const PEER: &str = "200";
pub const DEVICE: Uuid = Uuid::from_u128(0xd1);
pub const PEER_DEVICE: Uuid = Uuid::from_u128(0xd2);
pub const START_MS: i64 = 1_800_000_000_000;

pub struct Rig {
    pub server: Arc<Server>,
    pub local: Arc<MemoryLocal>,
    pub events: Arc<Recorder>,
    pub clock: Arc<TestClock>,
    pub engine: Arc<SyncEngine<MemoryLocal>>,
}

pub fn rig() -> Rig {
    rig_on(&[Tier::Direct, Tier::Relay, Tier::Pro])
}

pub fn rig_on(tiers: &[Tier]) -> Rig {
    let clock = Arc::new(TestClock::at(START_MS));
    let server = Arc::new(Server::new(tiers, clock.clone()));
    let channel = Arc::new(Channel::new(clock.clone()));
    let api = Arc::new(Api::with_exchange(channel, server.clone(), server.clone()));
    api.set_identity(Some(Identity {
        session_id: "session".into(),
        device_id: DEVICE,
        keys: Arc::new(DeviceKeys::from_seeds([1; 32], [2; 32])),
    }));
    let local = Arc::new(MemoryLocal::new(ME));
    let events = Arc::new(Recorder::default());
    let engine = Arc::new(SyncEngine::with_parts(
        api,
        local.clone(),
        events.clone(),
        Arc::new(TestCipher),
    ));
    engine.set_user(Some(ME.into()));
    let stopper = Arc::downgrade(&engine);
    server.when_done(move || {
        if let Some(engine) = stopper.upgrade() {
            engine.stop();
        }
    });
    Rig {
        server,
        local,
        events,
        clock,
        engine,
    }
}
