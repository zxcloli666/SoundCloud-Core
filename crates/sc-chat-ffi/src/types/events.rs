use sc_chat::{
    ChatStatus, ConvId, ConversationMarks, ConversationSummary, CoreEvent, Device,
    DeviceAddedEvent, GoneEvent, GoneMessage, Message, MessagesEvent, MessagesOrigin, PeerMark,
    SyncEvent, TypingEvent, UserId,
};

#[uniffi::remote(Enum)]
pub enum MessagesOrigin {
    Sync,
    History,
    Local,
}

#[uniffi::remote(Record)]
pub struct MessagesEvent {
    pub conversation_id: ConvId,
    pub origin: MessagesOrigin,
    pub messages: Vec<Message>,
}

#[uniffi::remote(Record)]
pub struct ConversationMarks {
    pub conversation_id: ConvId,
    pub marks: Vec<PeerMark>,
}

#[uniffi::remote(Record)]
pub struct SyncEvent {
    pub reset: bool,
    pub conversations: Vec<ConversationSummary>,
    pub marks: Vec<ConversationMarks>,
    pub requests: u32,
}

#[uniffi::remote(Record)]
pub struct TypingEvent {
    pub conversation_id: ConvId,
    pub users: Vec<UserId>,
}

#[uniffi::remote(Record)]
pub struct GoneEvent {
    pub conversations: Vec<ConvId>,
    pub messages: Vec<GoneMessage>,
}

#[uniffi::remote(Record)]
pub struct DeviceAddedEvent {
    pub device: Device,
}

#[uniffi::remote(Enum)]
pub enum CoreEvent {
    Status(ChatStatus),
    Sync(SyncEvent),
    Messages(MessagesEvent),
    Typing(TypingEvent),
    Gone(GoneEvent),
    DeviceAdded(DeviceAddedEvent),
}
