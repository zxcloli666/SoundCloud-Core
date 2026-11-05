use sc_chat::ChatPhase;

use super::Halt;
use crate::runtime::doubles::chat;

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
