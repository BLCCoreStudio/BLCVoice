#![forbid(unsafe_code)]

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPlatform {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl fmt::Display for DesktopPlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows => formatter.write_str("windows"),
            Self::MacOs => formatter.write_str("macos"),
            Self::Linux => formatter.write_str("linux"),
            Self::Other => formatter.write_str("other"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDisplayServer {
    X11,
    Wayland,
    Unknown,
}

impl fmt::Display for LinuxDisplayServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X11 => formatter.write_str("x11"),
            Self::Wayland => formatter.write_str("wayland"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopEnvironment {
    platform: DesktopPlatform,
    linux_display_server: LinuxDisplayServer,
}

impl DesktopEnvironment {
    #[must_use]
    pub const fn new(platform: DesktopPlatform, linux_display_server: LinuxDisplayServer) -> Self {
        Self {
            platform,
            linux_display_server,
        }
    }

    #[must_use]
    pub const fn platform(&self) -> DesktopPlatform {
        self.platform
    }

    #[must_use]
    pub const fn linux_display_server(&self) -> LinuxDisplayServer {
        self.linux_display_server
    }
}

/// Detect the effective Linux display server from standard desktop-session facts.
///
/// `XDG_SESSION_TYPE` wins when it explicitly identifies X11 or Wayland. When it
/// is absent or inconclusive, the display socket variables are used as evidence.
#[must_use]
pub fn detect_linux_display_server(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    x11_display: Option<&str>,
) -> LinuxDisplayServer {
    if let Some(session_type) = non_empty(xdg_session_type) {
        if session_type.eq_ignore_ascii_case("wayland") {
            return LinuxDisplayServer::Wayland;
        }
        if session_type.eq_ignore_ascii_case("x11") {
            return LinuxDisplayServer::X11;
        }
    }

    if non_empty(wayland_display).is_some() {
        return LinuxDisplayServer::Wayland;
    }
    if non_empty(x11_display).is_some() {
        return LinuxDisplayServer::X11;
    }

    LinuxDisplayServer::Unknown
}

/// Inspect the current process environment and return capability facts shared by
/// desktop subsystems such as shortcuts and text insertion.
#[must_use]
pub fn current_desktop_environment() -> DesktopEnvironment {
    current_desktop_environment_impl()
}

#[cfg(target_os = "windows")]
fn current_desktop_environment_impl() -> DesktopEnvironment {
    DesktopEnvironment::new(DesktopPlatform::Windows, LinuxDisplayServer::Unknown)
}

#[cfg(target_os = "macos")]
fn current_desktop_environment_impl() -> DesktopEnvironment {
    DesktopEnvironment::new(DesktopPlatform::MacOs, LinuxDisplayServer::Unknown)
}

#[cfg(target_os = "linux")]
fn current_desktop_environment_impl() -> DesktopEnvironment {
    let xdg_session_type = std::env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").ok();

    DesktopEnvironment::new(
        DesktopPlatform::Linux,
        detect_linux_display_server(
            xdg_session_type.as_deref(),
            wayland_display.as_deref(),
            x11_display.as_deref(),
        ),
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn current_desktop_environment_impl() -> DesktopEnvironment {
    DesktopEnvironment::new(DesktopPlatform::Other, LinuxDisplayServer::Unknown)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_type_takes_precedence_over_xwayland_display() {
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
    fn display_variables_are_fallback_evidence() {
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
    fn blank_values_do_not_create_false_capabilities() {
        assert_eq!(
            detect_linux_display_server(Some("  "), Some(""), Some("   ")),
            LinuxDisplayServer::Unknown
        );
    }
}
