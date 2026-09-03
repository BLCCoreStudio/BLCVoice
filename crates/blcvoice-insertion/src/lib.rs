#![forbid(unsafe_code)]

use core::fmt;

pub use blcvoice_platform::{DesktopEnvironment, DesktopPlatform, LinuxDisplayServer};

pub type InsertionEnvironment = DesktopEnvironment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionBackend {
    WindowsSendInput,
    MacOsQuartz,
    X11XTest,
    XdgRemoteDesktopEis,
}

impl fmt::Display for InsertionBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowsSendInput => formatter.write_str("windowsSendInput"),
            Self::MacOsQuartz => formatter.write_str("macOsQuartz"),
            Self::X11XTest => formatter.write_str("x11XTest"),
            Self::XdgRemoteDesktopEis => formatter.write_str("xdgRemoteDesktopEis"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionAuthorization {
    None,
    MacOsAccessibility,
    XdgRemoteDesktop,
}

impl fmt::Display for InsertionAuthorization {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("none"),
            Self::MacOsAccessibility => formatter.write_str("macOsAccessibility"),
            Self::XdgRemoteDesktop => formatter.write_str("xdgRemoteDesktop"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertionCapability {
    backend: InsertionBackend,
    authorization: InsertionAuthorization,
}

impl InsertionCapability {
    #[must_use]
    pub const fn new(backend: InsertionBackend, authorization: InsertionAuthorization) -> Self {
        Self {
            backend,
            authorization,
        }
    }

    #[must_use]
    pub const fn backend(&self) -> InsertionBackend {
        self.backend
    }

    #[must_use]
    pub const fn authorization(&self) -> InsertionAuthorization {
        self.authorization
    }

    /// Synthetic input APIs generally cannot prove that a target application's
    /// editable field actually changed. A successful backend receipt therefore
    /// means complete submission to the selected platform mechanism, not semantic
    /// verification of target content.
    #[must_use]
    pub const fn semantic_delivery_verifiable(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionCapabilityError {
    UnsupportedPlatform,
    UnknownLinuxDisplayServer,
}

impl fmt::Display for InsertionCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("text insertion is not supported on this platform")
            }
            Self::UnknownLinuxDisplayServer => formatter.write_str(
                "could not determine whether the Linux desktop session uses X11 or Wayland",
            ),
        }
    }
}

impl std::error::Error for InsertionCapabilityError {}

/// Resolve the least-invasive insertion backend allowed for one desktop
/// environment. Wayland deliberately selects a compositor-mediated portal/EIS
/// path and never falls back to X11, evdev, uinput, or root-only input injection.
pub const fn resolve_insertion_capability(
    environment: InsertionEnvironment,
) -> Result<InsertionCapability, InsertionCapabilityError> {
    match (
        environment.platform(),
        environment.linux_display_server(),
    ) {
        (DesktopPlatform::Windows, _) => Ok(InsertionCapability::new(
            InsertionBackend::WindowsSendInput,
            InsertionAuthorization::None,
        )),
        (DesktopPlatform::MacOs, _) => Ok(InsertionCapability::new(
            InsertionBackend::MacOsQuartz,
            InsertionAuthorization::MacOsAccessibility,
        )),
        (DesktopPlatform::Linux, LinuxDisplayServer::X11) => Ok(InsertionCapability::new(
            InsertionBackend::X11XTest,
            InsertionAuthorization::None,
        )),
        (DesktopPlatform::Linux, LinuxDisplayServer::Wayland) => Ok(InsertionCapability::new(
            InsertionBackend::XdgRemoteDesktopEis,
            InsertionAuthorization::XdgRemoteDesktop,
        )),
        (DesktopPlatform::Linux, LinuxDisplayServer::Unknown) => {
            Err(InsertionCapabilityError::UnknownLinuxDisplayServer)
        }
        (DesktopPlatform::Other, _) => Err(InsertionCapabilityError::UnsupportedPlatform),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InsertionReceipt {
    backend: InsertionBackend,
    submitted_utf8_bytes: usize,
}

impl InsertionReceipt {
    #[must_use]
    pub const fn complete(backend: InsertionBackend, submitted_utf8_bytes: usize) -> Self {
        Self {
            backend,
            submitted_utf8_bytes,
        }
    }

    #[must_use]
    pub const fn backend(&self) -> InsertionBackend {
        self.backend
    }

    #[must_use]
    pub const fn submitted_utf8_bytes(&self) -> usize {
        self.submitted_utf8_bytes
    }

    /// A receipt confirms that the backend accepted the complete text submission.
    /// It does not prove that the focused application's document mutated.
    #[must_use]
    pub const fn semantic_delivery_verified(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertionErrorKind {
    InvalidText,
    PermissionDenied,
    BackendUnavailable,
    PartialSubmission,
    BackendFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertionError {
    kind: InsertionErrorKind,
    message: String,
}

impl InsertionError {
    #[must_use]
    pub fn new(kind: InsertionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> InsertionErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for InsertionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for InsertionError {}

/// Platform adapters implement this contract. `Ok` is reserved for complete
/// submission according to the selected backend's own acknowledgement rules;
/// partial native API writes must return `InsertionErrorKind::PartialSubmission`.
pub trait TextInserter: Send {
    fn capability(&self) -> InsertionCapability;

    fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_uses_native_send_input() {
        let capability = resolve_insertion_capability(InsertionEnvironment::new(
            DesktopPlatform::Windows,
            LinuxDisplayServer::Unknown,
        ))
        .expect("Windows insertion should resolve");

        assert_eq!(capability.backend(), InsertionBackend::WindowsSendInput);
        assert_eq!(capability.authorization(), InsertionAuthorization::None);
        assert!(!capability.semantic_delivery_verifiable());
    }

    #[test]
    fn macos_requires_accessibility_authorization() {
        let capability = resolve_insertion_capability(InsertionEnvironment::new(
            DesktopPlatform::MacOs,
            LinuxDisplayServer::Unknown,
        ))
        .expect("macOS insertion should resolve");

        assert_eq!(capability.backend(), InsertionBackend::MacOsQuartz);
        assert_eq!(
            capability.authorization(),
            InsertionAuthorization::MacOsAccessibility
        );
    }

    #[test]
    fn linux_x11_uses_xtest() {
        let capability = resolve_insertion_capability(InsertionEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::X11,
        ))
        .expect("X11 insertion should resolve");

        assert_eq!(capability.backend(), InsertionBackend::X11XTest);
        assert_eq!(capability.authorization(), InsertionAuthorization::None);
    }

    #[test]
    fn linux_wayland_uses_portal_eis_without_raw_fallback() {
        let capability = resolve_insertion_capability(InsertionEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ))
        .expect("Wayland insertion should resolve");

        assert_eq!(
            capability.backend(),
            InsertionBackend::XdgRemoteDesktopEis
        );
        assert_eq!(
            capability.authorization(),
            InsertionAuthorization::XdgRemoteDesktop
        );
    }

    #[test]
    fn unknown_linux_session_is_explicitly_unavailable() {
        assert_eq!(
            resolve_insertion_capability(InsertionEnvironment::new(
                DesktopPlatform::Linux,
                LinuxDisplayServer::Unknown,
            )),
            Err(InsertionCapabilityError::UnknownLinuxDisplayServer)
        );
    }

    #[test]
    fn receipt_never_claims_semantic_target_verification() {
        let receipt = InsertionReceipt::complete(InsertionBackend::WindowsSendInput, 13);

        assert_eq!(receipt.submitted_utf8_bytes(), 13);
        assert!(!receipt.semantic_delivery_verified());
    }
}
