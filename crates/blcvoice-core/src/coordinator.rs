use std::error::Error;
use std::fmt;
use std::sync::{Mutex, MutexGuard};

use crate::{
    DictationSession, FailureStage, SessionEvent, SessionId, SessionState, Transition,
    TransitionError,
};

/// Immutable view of the session currently owned by the coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub id: SessionId,
    pub state: SessionState,
    pub failure_stage: Option<FailureStage>,
}

impl SessionSnapshot {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        self.state.is_terminal()
    }
}

impl From<&DictationSession> for SessionSnapshot {
    fn from(session: &DictationSession) -> Self {
        Self {
            id: session.id(),
            state: session.state(),
            failure_stage: session.failure_stage(),
        }
    }
}

/// One state transition applied to a coordinator-owned session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinatedTransition {
    pub session_id: SessionId,
    pub transition: Transition,
    pub snapshot: SessionSnapshot,
}

/// Failures at the single-active-session coordination boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCoordinatorError {
    SessionIdExhausted,
    NoActiveSession,
    Busy {
        active: SessionSnapshot,
    },
    StaleSession {
        supplied: SessionId,
        active: SessionId,
    },
    InvalidTransition(TransitionError),
}

impl fmt::Display for SessionCoordinatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionIdExhausted => formatter.write_str("dictation session id space exhausted"),
            Self::NoActiveSession => formatter.write_str("there is no active dictation session"),
            Self::Busy { active } => write!(
                formatter,
                "dictation session {} is still {:?}",
                active.id.get(),
                active.state
            ),
            Self::StaleSession { supplied, active } => write!(
                formatter,
                "dictation session {} is stale; current session is {}",
                supplied.get(),
                active.get()
            ),
            Self::InvalidTransition(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl Error for SessionCoordinatorError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidTransition(error) => Some(error),
            Self::SessionIdExhausted
            | Self::NoActiveSession
            | Self::Busy { .. }
            | Self::StaleSession { .. } => None,
        }
    }
}

#[derive(Debug)]
struct CoordinatorState {
    next_id: u64,
    current: Option<DictationSession>,
}

impl Default for CoordinatorState {
    fn default() -> Self {
        Self {
            next_id: 1,
            current: None,
        }
    }
}

/// Thread-safe owner of the single BLCVoice dictation session that may accept work.
///
/// Expensive capture, DSP and recognition work must happen outside this lock. Worker
/// threads carry the returned `SessionId` and submit lifecycle events back through
/// `transition`. A result from an older session is therefore rejected instead of being
/// applied to whichever session happens to be current when the worker finishes.
#[derive(Debug, Default)]
pub struct SessionCoordinator {
    state: Mutex<CoordinatorState>,
}

impl SessionCoordinator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a fresh session when no non-terminal session is active.
    ///
    /// A completed, failed or cancelled session is replaced by the new one. Session IDs
    /// are monotonically increasing for the lifetime of this coordinator.
    pub fn begin(&self) -> Result<SessionSnapshot, SessionCoordinatorError> {
        let mut state = self.lock();
        if let Some(current) = state.current.as_ref() {
            let snapshot = SessionSnapshot::from(current);
            if !snapshot.is_terminal() {
                return Err(SessionCoordinatorError::Busy { active: snapshot });
            }
        }

        let next_id = state
            .next_id
            .checked_add(1)
            .ok_or(SessionCoordinatorError::SessionIdExhausted)?;
        let id = SessionId::new(state.next_id);
        let session = DictationSession::new(id);
        let snapshot = SessionSnapshot::from(&session);

        state.next_id = next_id;
        state.current = Some(session);

        Ok(snapshot)
    }

    /// Return the current session without granting mutation access to its state machine.
    #[must_use]
    pub fn current(&self) -> Option<SessionSnapshot> {
        let state = self.lock();
        state.current.as_ref().map(SessionSnapshot::from)
    }

    /// Whether `session_id` still names the coordinator's current non-terminal session.
    #[must_use]
    pub fn accepts_work(&self, session_id: SessionId) -> bool {
        self.current()
            .is_some_and(|current| current.id == session_id && !current.is_terminal())
    }

    /// Apply an event only if it belongs to the currently-owned session.
    pub fn transition(
        &self,
        session_id: SessionId,
        event: SessionEvent,
    ) -> Result<CoordinatedTransition, SessionCoordinatorError> {
        let mut state = self.lock();
        let session = state
            .current
            .as_mut()
            .ok_or(SessionCoordinatorError::NoActiveSession)?;

        if session.id() != session_id {
            return Err(SessionCoordinatorError::StaleSession {
                supplied: session_id,
                active: session.id(),
            });
        }

        let transition = session
            .apply(event)
            .map_err(SessionCoordinatorError::InvalidTransition)?;
        let snapshot = SessionSnapshot::from(&*session);

        Ok(CoordinatedTransition {
            session_id,
            transition,
            snapshot,
        })
    }

    /// Cancel the current session if `session_id` still owns it.
    pub fn cancel(
        &self,
        session_id: SessionId,
    ) -> Result<CoordinatedTransition, SessionCoordinatorError> {
        self.transition(session_id, SessionEvent::Cancel)
    }

    fn lock(&self) -> MutexGuard<'_, CoordinatorState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use super::*;

    fn advance_to_transcribing(coordinator: &SessionCoordinator, id: SessionId) {
        for event in [
            SessionEvent::RecordingStarted,
            SessionEvent::RecordingStopped,
            SessionEvent::AudioFinalized,
        ] {
            coordinator
                .transition(id, event)
                .expect("test transition must be valid");
        }
    }

    #[test]
    fn allocates_monotonic_ids_after_terminal_sessions() {
        let coordinator = SessionCoordinator::new();
        let first = coordinator.begin().expect("first session must start");
        coordinator
            .cancel(first.id)
            .expect("first session must cancel");
        let second = coordinator.begin().expect("second session must start");

        assert_eq!(first.id.get(), 1);
        assert_eq!(second.id.get(), 2);
        assert_eq!(second.state, SessionState::Arming);
    }

    #[test]
    fn refuses_overlapping_non_terminal_sessions() {
        let coordinator = SessionCoordinator::new();
        let first = coordinator.begin().expect("first session must start");
        let error = coordinator
            .begin()
            .expect_err("second active session must be rejected");

        assert_eq!(error, SessionCoordinatorError::Busy { active: first });
        assert_eq!(coordinator.current(), Some(first));
    }

    #[test]
    fn rejects_late_events_from_replaced_sessions() {
        let coordinator = SessionCoordinator::new();
        let first = coordinator.begin().expect("first session must start");
        coordinator
            .cancel(first.id)
            .expect("first session must cancel");
        let second = coordinator.begin().expect("second session must start");

        let error = coordinator
            .transition(first.id, SessionEvent::RecordingStarted)
            .expect_err("stale session must not mutate current state");

        assert_eq!(
            error,
            SessionCoordinatorError::StaleSession {
                supplied: first.id,
                active: second.id,
            }
        );
        assert_eq!(coordinator.current(), Some(second));
    }

    #[test]
    fn concurrent_cancel_prevents_late_transcript_transition() {
        let coordinator = Arc::new(SessionCoordinator::new());
        let session = coordinator.begin().expect("session must start");
        advance_to_transcribing(&coordinator, session.id);

        let cancel_coordinator = Arc::clone(&coordinator);
        let session_id = session.id;
        thread::spawn(move || {
            cancel_coordinator
                .cancel(session_id)
                .expect("cancel must win while transcribing");
        })
        .join()
        .expect("cancel thread must not panic");

        let error = coordinator
            .transition(
                session.id,
                SessionEvent::TranscriptReady {
                    requires_transform: false,
                },
            )
            .expect_err("cancelled session must reject a late transcript");

        assert!(matches!(
            error,
            SessionCoordinatorError::InvalidTransition(TransitionError {
                state: SessionState::Cancelled,
                ..
            })
        ));
        assert_eq!(
            coordinator
                .current()
                .expect("session remains observable")
                .state,
            SessionState::Cancelled
        );
    }

    #[test]
    fn failure_stage_remains_visible_in_snapshot() {
        let coordinator = SessionCoordinator::new();
        let session = coordinator.begin().expect("session must start");
        let result = coordinator
            .transition(
                session.id,
                SessionEvent::Fail(FailureStage::SpeechRecognition),
            )
            .expect("non-terminal session may fail");

        assert_eq!(result.snapshot.state, SessionState::Failed);
        assert_eq!(
            result.snapshot.failure_stage,
            Some(FailureStage::SpeechRecognition)
        );
        assert!(!coordinator.accepts_work(session.id));
    }

    #[test]
    fn no_active_session_is_distinct_from_stale_session() {
        let coordinator = SessionCoordinator::new();
        let error = coordinator
            .transition(SessionId::new(99), SessionEvent::RecordingStarted)
            .expect_err("coordinator has no current session");

        assert_eq!(error, SessionCoordinatorError::NoActiveSession);
    }
}
