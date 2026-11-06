#![allow(dead_code)]

mod keys;

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder, Rgb, RgbImage};
use sc_chat::store::Store;
use sc_chat::{
    ChatError, ChatPhase, CoreEvent, KeyName, MediaUploadsEvent, Tier, TransportRequest,
    TransportResponse,
};
use sc_chat_ffi::{ChatClient, ChatKeys, ChatListener, ChatSetup, ChatTransport, NetFailure};

pub use keys::MemoryKeys;

#[derive(Default)]
pub struct Garbage(Mutex<Vec<TransportRequest>>);

impl Garbage {
    pub fn tiers_seen(&self) -> Vec<Tier> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.tier)
            .collect()
    }

    pub fn requests(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

#[async_trait]
impl ChatTransport for Garbage {
    fn tiers(&self) -> Vec<Tier> {
        vec![Tier::Direct, Tier::Relay]
    }

    async fn exchange(&self, request: TransportRequest) -> Result<TransportResponse, NetFailure> {
        self.0.lock().unwrap().push(request);
        Ok(TransportResponse {
            status: 200,
            body: vec![1, 2, 3],
            retry_after_ms: None,
        })
    }
}

type Hook = Box<dyn Fn(&CoreEvent) + Send + Sync>;

#[derive(Default)]
pub struct Recorder {
    seen: Mutex<Vec<CoreEvent>>,
    hook: Mutex<Option<Hook>>,
}

impl Recorder {
    pub fn statuses(&self) -> usize {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, CoreEvent::Status(_)))
            .count()
    }

    pub fn phases(&self) -> Vec<ChatPhase> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::Status(status) => Some(status.phase),
                _ => None,
            })
            .fold(Vec::new(), |mut phases, phase| {
                if phases.last() != Some(&phase) {
                    phases.push(phase);
                }
                phases
            })
    }

    pub fn uploads(&self) -> Vec<MediaUploadsEvent> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::MediaUploads(uploads) => Some(uploads.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn on(&self, hook: impl Fn(&CoreEvent) + Send + Sync + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }
}

impl ChatListener for Recorder {
    fn on_event(&self, event: CoreEvent) {
        if let Some(hook) = self.hook.lock().unwrap().as_ref() {
            hook(&event);
        }
        self.seen.lock().unwrap().push(event);
    }
}

pub struct Rig {
    pub client: Arc<ChatClient>,
    pub keys: Arc<MemoryKeys>,
    pub transport: Arc<Garbage>,
    pub events: Arc<Recorder>,
    pub dir: PathBuf,
}

impl Rig {
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("sc-chat-ffi-{}", uuid::Uuid::new_v4()));
        let keys = Arc::new(MemoryKeys::default());
        let transport = Arc::new(Garbage::default());
        let events = Arc::new(Recorder::default());
        let setup = ChatSetup {
            data_dir: dir.to_string_lossy().into_owned(),
            platform: "android".into(),
            device_name: "Test phone".into(),
            app_version: "0.0.0".into(),
        };
        let client =
            ChatClient::new(setup, keys.clone(), transport.clone(), events.clone()).unwrap();
        Self {
            client,
            keys,
            transport,
            events,
            dir,
        }
    }
}

impl Rig {
    pub async fn with_store(&self) {
        self.client.set_session(Some("session".into()));
        let started = Instant::now();
        while settle(self.client.uploads()).await.err() == Some(ChatError::NotReady) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the store opens"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub fn side_store(&self) -> Store {
        let key = self.keys.load(KeyName::Database).unwrap().unwrap();
        let master: [u8; 32] = key.try_into().unwrap();
        Store::open(&self.dir.join("chat.db"), &master).unwrap()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub async fn until(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < Duration::from_secs(10), "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub async fn settle<F: Future>(work: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .expect("the call answers")
}

pub fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let photo = RgbImage::from_fn(w, h, |x, y| {
        Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .write_image(&photo, w, h, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

pub fn summary(id: uuid::Uuid, kind: sc_chat::ConversationKind) -> sc_chat::ConversationSummary {
    sc_chat::ConversationSummary {
        id,
        kind,
        title: None,
        avatar_url: None,
        avatar_media_id: None,
        peer: None,
        role: sc_chat::MemberRole::Member,
        state: sc_chat::MemberState::Active,
        accepted: true,
        member_count: 3,
        last_seq: 0,
        last_at: 0,
        last_read_seq: 0,
        unread: 0,
        mentions: 0,
        preview: None,
        muted_until: None,
        notify: sc_chat::NotifyLevel::All,
        pinned_at: None,
        archived: false,
        ttl_s: None,
        room_code: None,
        history_locked: false,
        has_draft: false,
        pinged: false,
        slow_mode_s: 0,
    }
}

pub fn committed(store: &Store, id: uuid::Uuid, media: u128, thumb: Option<u128>) {
    let reserved = sc_chat::store::Reserved {
        media_id: uuid::Uuid::from_u128(media),
        thumb_media_id: thumb.map(uuid::Uuid::from_u128),
        presign_expires_at: i64::MAX,
    };
    store.upload_reserved(id, &reserved, 1).unwrap();
    store
        .upload_object_done(id, sc_chat::UploadObject::Image, 2)
        .unwrap();
    store
        .upload_object_done(id, sc_chat::UploadObject::Thumb, 3)
        .unwrap();
    store.upload_committed(id, 4).unwrap();
}
