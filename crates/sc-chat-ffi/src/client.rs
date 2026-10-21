use std::future::Future;
use std::sync::Arc;

use sc_chat::{
    ChatError, ChatResult, ChatStatus, ConvId, ConversationPage, ConversationSummary,
    CreateConversation, Device, DeviceId, HistoryPage, HistoryQuery, LinkClaimed, LinkCreated,
    LinkMode, LinkPoll, ListQuery, LoginBegin, LoginPoll, LoginTarget, Message, ReportInput,
    SendInput, Seq, ThreadSnapshot,
};
use tokio::runtime::Runtime;

use crate::platform::{ChatKeys, ChatListener, ChatTransport};
use crate::runtime::Chat;
use crate::setup::ChatSetup;

const THREADS: usize = 2;

#[derive(uniffi::Object)]
pub struct ChatClient {
    chat: Arc<Chat>,
    runtime: Option<Runtime>,
}

#[uniffi::export]
impl ChatClient {
    #[uniffi::constructor]
    pub fn new(
        setup: ChatSetup,
        keys: Arc<dyn ChatKeys>,
        transport: Arc<dyn ChatTransport>,
        listener: Arc<dyn ChatListener>,
    ) -> Result<Arc<Self>, ChatError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(THREADS)
            .thread_name("sc-chat")
            .enable_time()
            .build()
            .map_err(|_| ChatError::Internal)?;
        let handle = runtime.handle().clone();
        let chat = Arc::new(Chat::new(handle, setup, keys, transport, listener));
        runtime.spawn(chat.clone().run());
        Ok(Arc::new(Self {
            chat,
            runtime: Some(runtime),
        }))
    }

    pub fn status(&self) -> ChatStatus {
        self.chat.status()
    }

    pub fn set_session(&self, session_id: Option<String>) {
        self.chat.set_session(session_id);
    }

    pub fn set_visible(&self, visible: bool) {
        self.chat.set_visible(visible);
    }

    pub fn set_focus(&self, conversation_id: Option<ConvId>) {
        self.chat.set_focus(conversation_id);
    }

    pub fn network_changed(&self) {
        self.chat.network_changed();
    }

    pub async fn login_begin(&self, target: LoginTarget) -> Result<LoginBegin, ChatError> {
        self.run(|chat| async move { chat.login_begin(target).await })
            .await
    }

    pub async fn login_poll(&self, login_request_id: String) -> Result<LoginPoll, ChatError> {
        self.run(|chat| async move { chat.login_poll(&login_request_id).await })
            .await
    }

    pub async fn link_create(&self, mode: LinkMode) -> Result<LinkCreated, ChatError> {
        self.run(|chat| async move { chat.link_create(mode).await })
            .await
    }

    pub async fn link_claim(
        &self,
        mode: LinkMode,
        claim_token: String,
    ) -> Result<LinkClaimed, ChatError> {
        self.run(|chat| async move { chat.link_claim(mode, &claim_token).await })
            .await
    }

    pub async fn link_poll(&self, link_request_id: String) -> Result<LinkPoll, ChatError> {
        self.run(|chat| async move { chat.link_poll(&link_request_id).await })
            .await
    }

    pub async fn log_out(&self) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.log_out().await }).await
    }

    pub async fn conversations(&self, query: ListQuery) -> Result<ConversationPage, ChatError> {
        self.run(|chat| async move { chat.conversations(query).await })
            .await
    }

    pub async fn open_thread(&self, conversation_id: ConvId) -> Result<ThreadSnapshot, ChatError> {
        self.run(|chat| async move { chat.open(conversation_id).await })
            .await
    }

    pub async fn history(
        &self,
        conversation_id: ConvId,
        query: HistoryQuery,
    ) -> Result<HistoryPage, ChatError> {
        self.run(|chat| async move { chat.history(conversation_id, query).await })
            .await
    }

    pub async fn create_conversation(
        &self,
        input: CreateConversation,
    ) -> Result<ConversationSummary, ChatError> {
        self.run(|chat| async move { chat.create(input).await })
            .await
    }

    pub async fn send(&self, input: SendInput) -> Result<Message, ChatError> {
        self.run(|chat| async move { chat.send(input).await }).await
    }

    pub async fn mark_read(&self, conversation_id: ConvId, seq: Seq) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.mark_read(conversation_id, seq).await })
            .await
    }

    pub async fn set_ttl(
        &self,
        conversation_id: ConvId,
        ttl_s: Option<u32>,
    ) -> Result<ConversationSummary, ChatError> {
        self.run(|chat| async move { chat.set_ttl(conversation_id, ttl_s).await })
            .await
    }

    pub async fn report(&self, report: ReportInput) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.report(report).await })
            .await
    }

    pub async fn devices(&self) -> Result<Vec<Device>, ChatError> {
        self.run(|chat| async move { chat.devices().await }).await
    }

    pub async fn confirm_device(&self, device_id: DeviceId) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.confirm_device(device_id).await })
            .await
    }

    pub async fn device_not_me(&self, device_id: DeviceId) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.device_not_me(device_id).await })
            .await
    }

    pub async fn remove_device(&self, device_id: DeviceId) -> Result<(), ChatError> {
        self.run(|chat| async move { chat.remove_device(device_id).await })
            .await
    }
}

impl ChatClient {
    async fn run<T, F, Fut>(&self, call: F) -> ChatResult<T>
    where
        F: FnOnce(Arc<Chat>) -> Fut,
        Fut: Future<Output = ChatResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        self.chat
            .handle
            .spawn(call(self.chat.clone()))
            .await
            .unwrap_or(Err(ChatError::Internal))
    }
}

impl Drop for ChatClient {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
