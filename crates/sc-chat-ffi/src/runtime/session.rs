use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use sc_chat::ChatPhase;

use super::lock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMove {
    Stay,
    Restart,
    DropDevice,
}

#[derive(PartialEq, Eq)]
pub struct Seen {
    pub session_id: Option<String>,
    pub logged_out: bool,
    pub wipes: u64,
}

#[derive(Default)]
pub struct Session {
    current: Mutex<Option<String>>,
    logged_out: AtomicBool,
    wipes: AtomicU64,
}

impl Session {
    pub fn current(&self) -> Option<String> {
        lock(&self.current).clone()
    }

    pub fn replace(&self, session_id: Option<String>) -> Option<String> {
        std::mem::replace(&mut *lock(&self.current), session_id)
    }

    pub fn log_out(&self) {
        self.logged_out.store(true, Ordering::Release);
    }

    pub fn log_in(&self) {
        self.logged_out.store(false, Ordering::Release);
    }

    pub fn wiped(&self) {
        self.wipes.fetch_add(1, Ordering::AcqRel);
    }

    pub fn logged_out(&self) -> bool {
        self.logged_out.load(Ordering::Acquire)
    }

    pub fn seen(&self) -> Seen {
        Seen {
            session_id: self.current(),
            logged_out: self.logged_out(),
            wipes: self.wipes.load(Ordering::Acquire),
        }
    }
}

pub fn auth_move(
    was_signed_in: bool,
    signed_in: bool,
    changed: bool,
    phase: ChatPhase,
) -> AuthMove {
    if was_signed_in && !signed_in {
        return AuthMove::DropDevice;
    }
    let stuck = matches!(
        phase,
        ChatPhase::Unbound | ChatPhase::BoundOther | ChatPhase::Revoked
    );
    if changed || (signed_in && stuck) {
        AuthMove::Restart
    } else {
        AuthMove::Stay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wipe_between_a_log_out_and_the_same_login_still_changes_the_snapshot() {
        let session = Session::default();
        session.replace(Some("session".into()));
        let before = session.seen();
        session.log_out();
        session.wiped();
        session.log_in();
        assert!(session.seen() != before);
    }

    #[test]
    fn signing_out_drops_the_device() {
        assert_eq!(
            auth_move(true, false, true, ChatPhase::Ready),
            AuthMove::DropDevice
        );
    }

    #[test]
    fn a_new_session_or_a_stuck_phase_restarts() {
        assert_eq!(
            auth_move(false, true, true, ChatPhase::SignedOut),
            AuthMove::Restart
        );
        assert_eq!(
            auth_move(true, true, false, ChatPhase::Unbound),
            AuthMove::Restart
        );
        assert_eq!(
            auth_move(true, true, false, ChatPhase::Ready),
            AuthMove::Stay
        );
        assert_eq!(
            auth_move(false, false, false, ChatPhase::SignedOut),
            AuthMove::Stay
        );
    }
}
