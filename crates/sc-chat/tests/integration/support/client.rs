use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sc_chat::api::wire::{DeviceView, RegisterDevice, SendAnswer, SendMessage};
use sc_chat::api::{Api, Identity};
use sc_chat::channel::Channel;
use sc_chat::envelope::{self, Content, Post};
use sc_chat::outbox::{Outbox, OutboxItem};
use sc_chat::sc_chat_wire::prost::Message as _;
use sc_chat::store::Store;
use sc_chat::sync::{SyncEngine, SyncOutcome};
use sc_chat::{
    ChatResult, Clock, ConvId, ConversationSummary, CoreEvent, DeviceId, Events, GoneMessage,
    HistoryQuery, Message, MessageBody, Millis, RichText, SystemEvent, SystemKind, Tier,
    async_trait,
};
use uuid::Uuid;

use super::people::Person;
use super::transport::HttpTransport;
use super::{SOON, Stack, within};

pub struct WallClock;

#[async_trait]
impl Clock for WallClock {
    fn now_ms(&self) -> Millis {
        super::people::now_ms() as Millis
    }

    async fn sleep(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

#[derive(Default)]
pub struct Recorder(Mutex<Vec<CoreEvent>>);

impl Events for Recorder {
    fn emit(&self, event: CoreEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl Recorder {
    pub fn gone(&self) -> Vec<ConvId> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::Gone(gone) => Some(gone.conversations.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    pub fn devices_added(&self) -> Vec<DeviceId> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::DeviceAdded(added) => Some(added.device.id),
                _ => None,
            })
            .collect()
    }

    pub fn gone_messages(&self) -> Vec<GoneMessage> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CoreEvent::Gone(gone) => Some(gone.messages.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

pub struct Client {
    pub person: Person,
    pub transport: Arc<HttpTransport>,
    pub api: Arc<Api>,
    pub engine: SyncEngine,
    pub outbox: Outbox,
    pub events: Arc<Recorder>,
}

impl Client {
    pub async fn up(stack: &Stack, person: &Person) -> Self {
        let client = Self::on(vec![(Tier::Direct, stack.origin.clone())], person);
        client.api.config().await.expect("chat config answers");
        client.register().await.expect("the device registers");
        client
            .engine
            .devices()
            .await
            .expect("the device list loads");
        client.sync().await.expect("the first sync settles");
        client
    }

    pub fn on(routes: Vec<(Tier, String)>, person: &Person) -> Self {
        let master: [u8; 32] = sc_chat::sc_chat_wire::random_bytes().unwrap();
        let store = Arc::new(Store::open_in_memory(&master).unwrap());
        Self::with_store(routes, person, store)
    }

    pub fn on_disk(
        routes: Vec<(Tier, String)>,
        person: &Person,
        path: &Path,
        master: &[u8; 32],
    ) -> Self {
        let store = Arc::new(Store::open(path, master).unwrap());
        Self::with_store(routes, person, store)
    }

    pub fn with_store(routes: Vec<(Tier, String)>, person: &Person, store: Arc<Store>) -> Self {
        let transport = Arc::new(HttpTransport::new(routes));
        let channel = Arc::new(Channel::new(Arc::new(WallClock)));
        let api = Arc::new(Api::new(channel, transport.clone()));
        api.set_identity(Some(Identity {
            session_id: person.session_id.to_string(),
            device_id: person.device_id,
            keys: person.keys.clone(),
        }));
        store.set_me(&person.user_id).unwrap();
        let events = Arc::new(Recorder::default());
        let engine = SyncEngine::new(api.clone(), store.clone(), events.clone());
        engine.set_user(Some(person.user_id.clone()));
        Self {
            person: person.clone(),
            transport,
            api,
            engine,
            outbox: Outbox::new(store),
            events,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        self.engine.store()
    }

    pub async fn register(&self) -> ChatResult<DeviceView> {
        self.api
            .register_device(&RegisterDevice {
                platform: "linux".into(),
                name: format!("{} it", self.person.name),
                app_version: "0.0.0".into(),
                caps: Vec::new(),
            })
            .await
    }

    pub async fn sync(&self) -> ChatResult<SyncOutcome> {
        let plan = self.engine.next_plan()?;
        let outcome = self.engine.step(plan).await?;
        if outcome == SyncOutcome::Reset {
            self.engine.reconcile().await?;
        }
        Ok(outcome)
    }

    pub async fn sync_until(&self, what: &str, done: impl Fn(&Self) -> bool) {
        self.sync_within(what, SOON, done).await;
    }

    pub async fn sync_within(&self, what: &str, limit: Duration, done: impl Fn(&Self) -> bool) {
        within(what, limit, || async {
            self.sync().await.expect("sync answers");
            done(self)
        })
        .await;
    }

    pub async fn send(&self, conversation_id: ConvId, text: &str) -> ChatResult<usize> {
        self.queue(conversation_id, text)?;
        self.engine.flush_outbox().await
    }

    pub fn queue(&self, conversation_id: ConvId, text: &str) -> ChatResult<Uuid> {
        let content = Content::Post(Post {
            text: RichText {
                text: text.to_owned(),
                spans: Vec::new(),
            },
            mentions: Vec::new(),
            embeds: Vec::new(),
            reply_to: None,
            forwarded_from: None,
        });
        let now = self.api.channel().now_ms();
        let client_id = Uuid::new_v4();
        self.outbox.push(OutboxItem {
            client_id,
            conversation_id,
            class: content.class(),
            target_seq: content.target(),
            envelope: envelope::encode(&content)?.encode_to_vec(),
            created_at: now,
            attempts: 0,
            next_at: now,
        })?;
        Ok(client_id)
    }

    pub async fn raw_post(&self, conversation_id: ConvId) -> ChatResult<SendAnswer> {
        let view = self.api.conversation(conversation_id).await?;
        self.raw_post_at(conversation_id, view.key_epoch).await
    }

    pub async fn raw_room_post(&self, conversation_id: ConvId) -> ChatResult<SendAnswer> {
        let keys = self.api.keys(conversation_id, None).await?;
        let epoch = keys.iter().map(|key| key.epoch).max().unwrap_or_default();
        self.raw_post_at(conversation_id, epoch).await
    }

    async fn raw_post_at(&self, conversation_id: ConvId, epoch: i32) -> ChatResult<SendAnswer> {
        let message = SendMessage {
            client_id: Uuid::new_v4(),
            class: "post".into(),
            epoch,
            target_seq: None,
            body: vec![7; 64],
        };
        self.api.send(conversation_id, &message).await
    }

    pub fn messages(&self, conversation_id: ConvId) -> Vec<Message> {
        let query = HistoryQuery {
            before: None,
            after: None,
            limit: 200,
        };
        self.store()
            .messages(conversation_id, &query)
            .map(|page| page.messages)
            .unwrap_or_default()
    }

    pub fn texts(&self, conversation_id: ConvId) -> Vec<String> {
        self.messages(conversation_id)
            .into_iter()
            .filter_map(|message| match message.body {
                MessageBody::Text(rich) => Some(rich.text),
                MessageBody::Deleted => Some("<deleted>".into()),
                MessageBody::Locked => Some("<locked>".into()),
                MessageBody::System(_) => None,
                MessageBody::Unknown => Some("<unknown>".into()),
            })
            .collect()
    }

    pub fn systems(&self, conversation_id: ConvId) -> Vec<SystemEvent> {
        self.messages(conversation_id)
            .into_iter()
            .filter_map(|message| match message.body {
                MessageBody::System(event) => Some(event),
                _ => None,
            })
            .collect()
    }

    pub fn has_system(&self, conversation_id: ConvId, kind: SystemKind, target: &str) -> bool {
        self.systems(conversation_id)
            .iter()
            .any(|event| event.kind == kind && event.target.as_deref() == Some(target))
    }

    pub fn message(&self, conversation_id: ConvId, text: &str) -> Option<Message> {
        self.messages(conversation_id)
            .into_iter()
            .find(|message| matches!(&message.body, MessageBody::Text(rich) if rich.text == text))
    }

    pub fn conversation(&self, conversation_id: ConvId) -> Option<ConversationSummary> {
        self.store().conversation(conversation_id).ok().flatten()
    }
}
