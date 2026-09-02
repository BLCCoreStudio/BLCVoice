#![forbid(unsafe_code)]

use core::fmt;

/// Default BLCVoice dictation shortcut in the XDG shortcuts syntax.
pub const DEFAULT_DICTATION_TRIGGER: &str = "CTRL+SHIFT+space";

/// Stable application-level identifier used by native shortcut backends.
pub const DICTATION_SHORTCUT_ID: &str = "dictation.toggle";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DictationShortcutMode {
    /// Press once to start recording, press again to stop.
    #[default]
    Toggle,
    /// Hold the shortcut to record, release it to stop.
    PushToTalk,
}

impl fmt::Display for DictationShortcutMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toggle => formatter.write_str("toggle"),
            Self::PushToTalk => formatter.write_str("pushToTalk"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutPhase {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutDecision {
    StartDictation,
    StopDictation,
    Ignore,
}

/// Runtime-independent state machine for one dictation shortcut.
///
/// Native backends may deliver repeated key-down events while a key remains
/// physically held. `ShortcutController` suppresses those repeats so toggle
/// mode cannot immediately start and stop from keyboard auto-repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutController {
    mode: DictationShortcutMode,
    key_is_down: bool,
    dictation_requested: bool,
}

impl ShortcutController {
    #[must_use]
    pub const fn new(mode: DictationShortcutMode) -> Self {
        Self {
            mode,
            key_is_down: false,
            dictation_requested: false,
        }
    }

    #[must_use]
    pub const fn mode(&self) -> DictationShortcutMode {
        self.mode
    }

    #[must_use]
    pub const fn dictation_requested(&self) -> bool {
        self.dictation_requested
    }

    /// Apply a native shortcut press/release event.
    #[must_use]
    pub fn handle(&mut self, phase: ShortcutPhase) -> ShortcutDecision {
        match phase {
            ShortcutPhase::Pressed => self.handle_pressed(),
            ShortcutPhase::Released => self.handle_released(),
        }
    }

    /// Reconcile shortcut state when dictation is cancelled or terminated by
    /// another subsystem. Physical key state is preserved so a held
    /// push-to-talk key cannot immediately restart the session.
    pub fn force_idle(&mut self) {
        self.dictation_requested = false;
    }

    /// Change interaction mode only while no dictation is requested and no
    /// shortcut key is physically held.
    pub fn set_mode(&mut self, mode: DictationShortcutMode) -> Result<(), ShortcutModeError> {
        if self.dictation_requested || self.key_is_down {
            return Err(ShortcutModeError::Busy);
        }

        self.mode = mode;
        Ok(())
    }

    fn handle_pressed(&mut self) -> ShortcutDecision {
        if self.key_is_down {
            return ShortcutDecision::Ignore;
        }
        self.key_is_down = true;

        match self.mode {
            DictationShortcutMode::Toggle => {
                self.dictation_requested = !self.dictation_requested;
                if self.dictation_requested {
                    ShortcutDecision::StartDictation
                } else {
                    ShortcutDecision::StopDictation
                }
            }
            DictationShortcutMode::PushToTalk => {
                if self.dictation_requested {
                    ShortcutDecision::Ignore
                } else {
                    self.dictation_requested = true;
                    ShortcutDecision::StartDictation
                }
            }
        }
    }

    fn handle_released(&mut self) -> ShortcutDecision {
        if !self.key_is_down {
            return ShortcutDecision::Ignore;
        }
        self.key_is_down = false;

        match self.mode {
            DictationShortcutMode::Toggle => ShortcutDecision::Ignore,
            DictationShortcutMode::PushToTalk => {
                if self.dictation_requested {
                    self.dictation_requested = false;
                    ShortcutDecision::StopDictation
                } else {
                    ShortcutDecision::Ignore
                }
            }
        }
    }
}

impl Default for ShortcutController {
    fn default() -> Self {
        Self::new(DictationShortcutMode::default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutModeError {
    Busy,
}

impl fmt::Display for ShortcutModeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str(
                "shortcut mode cannot change while the shortcut is held or dictation is active",
            ),
        }
    }
}

impl std::error::Error for ShortcutModeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_mode_starts_and_stops_on_distinct_presses() {
        let mut controller = ShortcutController::default();

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StopDictation
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );
        assert!(!controller.dictation_requested());
    }

    #[test]
    fn toggle_mode_ignores_key_repeat() {
        let mut controller = ShortcutController::default();

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::Ignore
        );
        assert!(controller.dictation_requested());
    }

    #[test]
    fn push_to_talk_stops_on_release() {
        let mut controller = ShortcutController::new(DictationShortcutMode::PushToTalk);

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Released),
            ShortcutDecision::StopDictation
        );
        assert!(!controller.dictation_requested());
    }

    #[test]
    fn push_to_talk_repeat_does_not_restart() {
        let mut controller = ShortcutController::new(DictationShortcutMode::PushToTalk);

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::Ignore
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Released),
            ShortcutDecision::StopDictation
        );
    }

    #[test]
    fn force_idle_keeps_held_push_to_talk_key_from_restarting() {
        let mut controller = ShortcutController::new(DictationShortcutMode::PushToTalk);

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        controller.force_idle();

        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::Ignore
        );
        assert_eq!(
            controller.handle(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );
        assert!(!controller.dictation_requested());
    }

    #[test]
    fn mode_change_is_rejected_while_active() {
        let mut controller = ShortcutController::default();
        assert_eq!(
            controller.handle(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );

        assert_eq!(
            controller.set_mode(DictationShortcutMode::PushToTalk),
            Err(ShortcutModeError::Busy)
        );
    }

    #[test]
    fn default_policy_is_toggle_to_talk() {
        assert_eq!(
            ShortcutController::default().mode(),
            DictationShortcutMode::Toggle
        );
        assert_eq!(DEFAULT_DICTATION_TRIGGER, "CTRL+SHIFT+space");
    }
}
