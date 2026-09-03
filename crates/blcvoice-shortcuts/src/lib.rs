#![forbid(unsafe_code)]

use core::fmt;

use blcvoice_platform::{DesktopEnvironment, current_desktop_environment};
pub use blcvoice_platform::{DesktopPlatform, LinuxDisplayServer, detect_linux_display_server};

pub type ShortcutEnvironment = DesktopEnvironment;

/// Default BLCVoice dictation shortcut in the XDG shortcuts syntax.
pub const DEFAULT_DICTATION_TRIGGER: &str = "CTRL+SHIFT+space";

/// Stable application-level identifier used by native shortcut backends.
pub const DICTATION_SHORTCUT_ID: &str = "dictation.toggle";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutBackend {
    NativeGlobalHotkey,
    X11GlobalHotkey,
    XdgDesktopPortal,
}

impl fmt::Display for ShortcutBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NativeGlobalHotkey => formatter.write_str("nativeGlobalHotkey"),
            Self::X11GlobalHotkey => formatter.write_str("x11GlobalHotkey"),
            Self::XdgDesktopPortal => formatter.write_str("xdgDesktopPortal"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutCapabilityError {
    UnsupportedPlatform,
    UnknownLinuxDisplayServer,
}

impl fmt::Display for ShortcutCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("global shortcuts are not supported on this platform")
            }
            Self::UnknownLinuxDisplayServer => formatter.write_str(
                "could not determine whether the Linux desktop session uses X11 or Wayland",
            ),
        }
    }
}

impl std::error::Error for ShortcutCapabilityError {}

/// Resolve the non-invasive shortcut backend allowed for one desktop environment.
///
/// Wayland deliberately maps to the compositor-mediated XDG Desktop Portal path;
/// it must never silently fall back to raw global input capture.
pub const fn resolve_shortcut_backend(
    environment: ShortcutEnvironment,
) -> Result<ShortcutBackend, ShortcutCapabilityError> {
    match (environment.platform, environment.linux_display_server) {
        (DesktopPlatform::Windows | DesktopPlatform::MacOs, _) => {
            Ok(ShortcutBackend::NativeGlobalHotkey)
        }
        (DesktopPlatform::Linux, LinuxDisplayServer::X11) => Ok(ShortcutBackend::X11GlobalHotkey),
        (DesktopPlatform::Linux, LinuxDisplayServer::Wayland) => {
            Ok(ShortcutBackend::XdgDesktopPortal)
        }
        (DesktopPlatform::Linux, LinuxDisplayServer::Unknown) => {
            Err(ShortcutCapabilityError::UnknownLinuxDisplayServer)
        }
        (DesktopPlatform::Other, _) => Err(ShortcutCapabilityError::UnsupportedPlatform),
    }
}

/// Inspect the current process environment and return the platform facts needed
/// by the shortcut backend resolver.
#[must_use]
pub fn current_shortcut_environment() -> ShortcutEnvironment {
    current_desktop_environment()
}

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
    fn native_platforms_resolve_to_native_global_hotkeys() {
        for platform in [DesktopPlatform::Windows, DesktopPlatform::MacOs] {
            assert_eq!(
                resolve_shortcut_backend(ShortcutEnvironment::new(
                    platform,
                    LinuxDisplayServer::Unknown,
                )),
                Ok(ShortcutBackend::NativeGlobalHotkey)
            );
        }
    }

    #[test]
    fn linux_x11_resolves_to_x11_backend() {
        assert_eq!(
            resolve_shortcut_backend(ShortcutEnvironment::new(
                DesktopPlatform::Linux,
                LinuxDisplayServer::X11,
            )),
            Ok(ShortcutBackend::X11GlobalHotkey)
        );
    }

    #[test]
    fn linux_wayland_resolves_to_portal_backend() {
        assert_eq!(
            resolve_shortcut_backend(ShortcutEnvironment::new(
                DesktopPlatform::Linux,
                LinuxDisplayServer::Wayland,
            )),
            Ok(ShortcutBackend::XdgDesktopPortal)
        );
    }

    #[test]
    fn unknown_linux_session_is_an_explicit_capability_error() {
        assert_eq!(
            resolve_shortcut_backend(ShortcutEnvironment::new(
                DesktopPlatform::Linux,
                LinuxDisplayServer::Unknown,
            )),
            Err(ShortcutCapabilityError::UnknownLinuxDisplayServer)
        );
    }

    #[test]
    fn xdg_session_type_takes_precedence_over_xwayland_display() {
        assert_eq!(
            detect_linux_display_server(Some("wayland"), Some("wayland-0"), Some(":0")),
            LinuxDisplayServer::Wayland
        );
        assert_eq!(
            detect_linux_display_server(Some("x11"), Some("wayland-0"), Some(":0")),
            LinuxDisplayServer::X11
        );
    }

    #[test]
    fn display_variables_are_used_when_session_type_is_missing() {
        assert_eq!(
            detect_linux_display_server(None, Some("wayland-1"), Some(":0")),
            LinuxDisplayServer::Wayland
        );
        assert_eq!(
            detect_linux_display_server(None, None, Some(":1")),
            LinuxDisplayServer::X11
        );
        assert_eq!(
            detect_linux_display_server(Some("tty"), None, None),
            LinuxDisplayServer::Unknown
        );
    }

    #[test]
    fn blank_environment_values_do_not_create_false_capabilities() {
        assert_eq!(
            detect_linux_display_server(Some("  "), Some(""), Some("   ")),
            LinuxDisplayServer::Unknown
        );
    }

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
