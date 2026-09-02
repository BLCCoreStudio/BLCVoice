use blcvoice_shortcuts::{
    DesktopPlatform, ShortcutCapabilityError, ShortcutEnvironment, current_shortcut_environment,
    resolve_shortcut_backend,
};
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCapabilityDto {
    platform: String,
    linux_display_server: Option<String>,
    selected_backend: Option<String>,
    registration_implemented: bool,
    selection_error: Option<ShortcutCapabilityErrorDto>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCapabilityErrorDto {
    code: &'static str,
    message: String,
}

#[tauri::command]
pub fn shortcut_capability() -> ShortcutCapabilityDto {
    shortcut_capability_for(current_shortcut_environment())
}

fn shortcut_capability_for(environment: ShortcutEnvironment) -> ShortcutCapabilityDto {
    let platform = environment.platform();
    let linux_display_server = match platform {
        DesktopPlatform::Linux => Some(environment.linux_display_server().to_string()),
        DesktopPlatform::Windows | DesktopPlatform::MacOs | DesktopPlatform::Other => None,
    };

    match resolve_shortcut_backend(environment) {
        Ok(backend) => ShortcutCapabilityDto {
            platform: platform.to_string(),
            linux_display_server,
            selected_backend: Some(backend.to_string()),
            registration_implemented: false,
            selection_error: None,
        },
        Err(error) => ShortcutCapabilityDto {
            platform: platform.to_string(),
            linux_display_server,
            selected_backend: None,
            registration_implemented: false,
            selection_error: Some(ShortcutCapabilityErrorDto {
                code: shortcut_capability_error_code(error),
                message: error.to_string(),
            }),
        },
    }
}

const fn shortcut_capability_error_code(error: ShortcutCapabilityError) -> &'static str {
    match error {
        ShortcutCapabilityError::UnsupportedPlatform => "unsupported_platform",
        ShortcutCapabilityError::UnknownLinuxDisplayServer => "unknown_linux_display_server",
    }
}

#[cfg(test)]
mod tests {
    use blcvoice_shortcuts::LinuxDisplayServer;

    use super::*;

    #[test]
    fn wayland_reports_portal_backend_without_claiming_registration() {
        let capability = shortcut_capability_for(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ));

        assert_eq!(capability.platform, "linux");
        assert_eq!(capability.linux_display_server.as_deref(), Some("wayland"));
        assert_eq!(
            capability.selected_backend.as_deref(),
            Some("xdgDesktopPortal")
        );
        assert!(!capability.registration_implemented);
        assert_eq!(capability.selection_error, None);
    }

    #[test]
    fn unknown_linux_session_reports_typed_selection_error() {
        let capability = shortcut_capability_for(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Unknown,
        ));

        assert_eq!(capability.selected_backend, None);
        assert!(!capability.registration_implemented);
        let error = capability.selection_error.expect("selection should fail");
        assert_eq!(error.code, "unknown_linux_display_server");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn non_linux_desktop_omits_linux_display_server() {
        let capability = shortcut_capability_for(ShortcutEnvironment::new(
            DesktopPlatform::Windows,
            LinuxDisplayServer::Unknown,
        ));

        assert_eq!(capability.platform, "windows");
        assert_eq!(capability.linux_display_server, None);
        assert_eq!(
            capability.selected_backend.as_deref(),
            Some("nativeGlobalHotkey")
        );
        assert_eq!(capability.selection_error, None);
    }
}
