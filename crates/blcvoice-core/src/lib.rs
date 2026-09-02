#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// Stable identifier for a single dictation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The bounded lifecycle of a dictation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Arming,
    Recording,
    FinalizingAudio,
    Transcribing,
    Transforming,
    Inserting,
    Completed,
    Failed,
    Cancelled,
}

impl SessionState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Pipeline stage responsible for a terminal failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStage {
    AudioCapture,
    SpeechDetection,
    SpeechRecognition,
    Transformation,
    TargetResolution,
    TextInsertion,
    Internal,
}

/// Events accepted by the session state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    RecordingStarted,
    RecordingStopped,
    AudioFinalized,
    TranscriptReady { requires_transform: bool },
    TransformFinished,
    InsertionDelivered,
    Fail(FailureStage),
    Cancel,
}

/// A successfully applied state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub from: SessionState,
    pub to: SessionState,
    pub event: SessionEvent,
}

/// Error returned when an event is invalid for the current session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub state: SessionState,
    pub event: SessionEvent,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "event {:?} is invalid while session is {:?}",
            self.event, self.state
        )
    }
}

impl Error for TransitionError {}

/// Platform-independent state for one dictation operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationSession {
    id: SessionId,
    state: SessionState,
    failure_stage: Option<FailureStage>,
}

impl DictationSession {
    #[must_use]
    pub const fn new(id: SessionId) -> Self {
        Self {
            id,
            state: SessionState::Arming,
            failure_stage: None,
        }
    }

    #[must_use]
    pub const fn id(&self) -> SessionId {
        self.id
    }

    #[must_use]
    pub const fn state(&self) -> SessionState {
        self.state
    }

    #[must_use]
    pub const fn failure_stage(&self) -> Option<FailureStage> {
        self.failure_stage
    }

    /// Applies one lifecycle event without mutating the session on invalid input.
    pub fn apply(&mut self, event: SessionEvent) -> Result<Transition, TransitionError> {
        let from = self.state;

        let to = match (from, event) {
            (SessionState::Arming, SessionEvent::RecordingStarted) => SessionState::Recording,
            (SessionState::Recording, SessionEvent::RecordingStopped) => {
                SessionState::FinalizingAudio
            }
            (SessionState::FinalizingAudio, SessionEvent::AudioFinalized) => {
                SessionState::Transcribing
            }
            (
                SessionState::Transcribing,
                SessionEvent::TranscriptReady {
                    requires_transform: true,
                },
            ) => SessionState::Transforming,
            (
                SessionState::Transcribing,
                SessionEvent::TranscriptReady {
                    requires_transform: false,
                },
            ) => SessionState::Inserting,
            (SessionState::Transforming, SessionEvent::TransformFinished) => SessionState::Inserting,
            (SessionState::Inserting, SessionEvent::InsertionDelivered) => SessionState::Completed,
            (state, SessionEvent::Fail(stage)) if !state.is_terminal() => {
                self.failure_stage = Some(stage);
                SessionState::Failed
            }
            (state, SessionEvent::Cancel) if !state.is_terminal() => SessionState::Cancelled,
            _ => return Err(TransitionError { state: from, event }),
        };

        self.state = to;
        Ok(Transition { from, to, event })
    }
}

/// Human-readable foundation status used by the desktop shell during bootstrap.
#[must_use]
pub fn status_line() -> String {
    format!(
        "BLCVoice core {} · dictation state machine ready",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_the_pipeline_with_transform() {
        let mut session = DictationSession::new(SessionId::new(1));
        let events = [
            SessionEvent::RecordingStarted,
            SessionEvent::RecordingStopped,
            SessionEvent::AudioFinalized,
            SessionEvent::TranscriptReady {
                requires_transform: true,
            },
            SessionEvent::TransformFinished,
            SessionEvent::InsertionDelivered,
        ];

        for event in events {
            session.apply(event).expect("happy-path event must be valid");
        }

        assert_eq!(session.state(), SessionState::Completed);
        assert_eq!(session.failure_stage(), None);
    }

    #[test]
    fn can_skip_optional_transform_stage() {
        let mut session = DictationSession::new(SessionId::new(2));
        for event in [
            SessionEvent::RecordingStarted,
            SessionEvent::RecordingStopped,
            SessionEvent::AudioFinalized,
            SessionEvent::TranscriptReady {
                requires_transform: false,
            },
            SessionEvent::InsertionDelivered,
        ] {
            session.apply(event).expect("event must be valid");
        }

        assert_eq!(session.state(), SessionState::Completed);
    }

    #[test]
    fn rejects_invalid_transition_without_mutation() {
        let mut session = DictationSession::new(SessionId::new(3));
        let error = session
            .apply(SessionEvent::AudioFinalized)
            .expect_err("arming cannot finalize audio");

        assert_eq!(error.state, SessionState::Arming);
        assert_eq!(session.state(), SessionState::Arming);
    }

    #[test]
    fn preserves_failure_stage() {
        let mut session = DictationSession::new(SessionId::new(4));
        session
            .apply(SessionEvent::Fail(FailureStage::AudioCapture))
            .expect("non-terminal session may fail");

        assert_eq!(session.state(), SessionState::Failed);
        assert_eq!(session.failure_stage(), Some(FailureStage::AudioCapture));
    }

    #[test]
    fn terminal_state_rejects_further_events() {
        let mut session = DictationSession::new(SessionId::new(5));
        session
            .apply(SessionEvent::Cancel)
            .expect("non-terminal session may be cancelled");

        assert!(session.apply(SessionEvent::RecordingStarted).is_err());
        assert_eq!(session.state(), SessionState::Cancelled);
    }
}
