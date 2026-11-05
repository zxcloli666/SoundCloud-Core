use std::sync::{Arc, OnceLock, Weak};

use sc_chat::{ChatPhase, CoreEvent};

use super::{Chat, Halt};
use crate::platform::ChatListener;
use crate::runtime::doubles::{Origin, chat, chat_at};

#[derive(Default)]
struct SignsOutOnPause(OnceLock<Weak<Chat>>);

impl ChatListener for SignsOutOnPause {
    fn on_event(&self, event: CoreEvent) {
        if let CoreEvent::Status(status) = event
            && status.send_paused
            && let Some(chat) = self.0.get().and_then(Weak::upgrade)
        {
            chat.session.replace(None);
        }
    }
}

#[tokio::test]
async fn a_restart_the_attempt_already_saw_does_not_cut_it_short() {
    let (chat, _scratch) = chat();
    chat.session.replace(Some("session".into()));
    let seen = chat.session.seen();
    chat.restart();

    let halt = chat.bring_up(&seen, &mut 0).await.err();

    assert_eq!(halt, Some(Halt::Phase(ChatPhase::KeychainDenied)));
}

#[tokio::test]
async fn a_sign_out_after_the_attempt_started_cuts_it_short() {
    let (chat, _scratch) = chat();
    chat.session.replace(Some("session".into()));
    let seen = chat.session.seen();
    chat.session.replace(None);

    let halt = chat.bring_up(&seen, &mut 0).await.err();

    assert_eq!(halt, Some(Halt::Restart));
}

#[tokio::test]
async fn a_sign_out_as_the_config_lands_registers_no_device() {
    let origin = Arc::new(Origin::default());
    let listener = Arc::new(SignsOutOnPause::default());
    let (chat, _scratch) = chat_at(origin.clone(), listener.clone());
    let _ = listener.0.set(Arc::downgrade(&chat));
    chat.session.replace(Some("session".into()));
    let seen = chat.session.seen();

    let halt = chat.bring_up(&seen, &mut 0).await.err();

    assert_eq!(halt, Some(Halt::Restart));
    assert_eq!(origin.calls(), ["GET /chat/config"]);
}
