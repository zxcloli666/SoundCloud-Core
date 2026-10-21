use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use sc_chat::ChatPhase;

use super::lock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMove {
    Stay,
    Restart,
    DropDevice,
}

#[derive(Default)]
pub struct Session {
    current: Mutex<Option<String>>,
    logged_out: AtomicBool,
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

    pub fn logged_out(&self) -> bool {
        self.logged_out.load(Ordering::Acquire)
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
