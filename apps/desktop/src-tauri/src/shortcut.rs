use std::sync::{Mutex, MutexGuard};

use blcvoice_shortcuts::{
    DEFAULT_DICTATION_TRIGGER, DICTATION_SHORTCUT_ID, DesktopPlatform, ShortcutBackend,
    ShortcutCapabilityError, ShortcutController, ShortcutDecision, ShortcutEnvironment,
    ShortcutPhase, current_shortcut_environment, resolve_shortcut_backend,
};
use serde::Serialize;
use tauri::{App, AppHandle, Emitter, Manager, Runtime, State};

use crate::coordinator::ShortcutDictationCoordinator;

#[cfg(target_os = "linux")]
use ashpd::desktop::{
    CreateSessionOptions,
    global_shortcuts::{
        BindShortcutsOptions, ConfigureShortcutsOptions, GlobalShortcuts, NewShortcut,
    },
};
#[cfg(target_os = "linux")]
use futures_util::{StreamExt, stream};
#[cfg(desktop)]
use tauri_plugin_global_shortcut::ShortcutState;

pub const SHORTCUT_DECISION_EVENT: &str = "blcvoice://shortcut-decision";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutRegistrationState {
    Pending,
    Registering,
    ConfigurationRequired,
    Registered,
    Failed,
    Unavailable,
}

impl ShortcutRegistrationState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Registering => "registering",
            Self::ConfigurationRequired => "configuration required",
            Self::Registered => "registered",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug)]
struct ShortcutServiceState {
    environment: ShortcutEnvironment,
    backend: Option<ShortcutBackend>,
    selection_error: Option<ShortcutCapabilityError>,
    registration_state: ShortcutRegistrationState,
    registration_error: Option<String>,
    bound_trigger_description: Option<String>,
    controller: ShortcutController,
}

#[derive(Debug)]
pub struct ShortcutService {
    state: Mutex<ShortcutServiceState>,
}

impl ShortcutService {
    #[must_use]
    pub fn production() -> Self {
        Self::for_environment(current_shortcut_environment())
    }

    fn for_environment(environment: ShortcutEnvironment) -> Self {
        let (backend, selection_error, registration_state) =
            match resolve_shortcut_backend(environment) {
                Ok(backend) => (Some(backend), None, ShortcutRegistrationState::Pending),
                Err(error) => (None, Some(error), ShortcutRegistrationState::Unavailable),
            };

        Self {
            state: Mutex::new(ShortcutServiceState {
                environment,
                backend,
                selection_error,
                registration_state,
                registration_error: None,
                bound_trigger_description: None,
                controller: ShortcutController::default(),
            }),
        }
    }

    fn backend(&self) -> Option<ShortcutBackend> {
        self.lock_state().backend
    }

    fn mark_registering(&self) {
        let mut state = self.lock_state();
        if state.backend.is_some() {
            state.registration_state = ShortcutRegistrationState::Registering;
            state.registration_error = None;
            state.bound_trigger_description = None;
        }
    }

    fn mark_configuration_required(&self) {
        let mut state = self.lock_state();
        if state.backend.is_some() {
            state.registration_state = ShortcutRegistrationState::ConfigurationRequired;
            state.registration_error = None;
            state.bound_trigger_description = None;
            state.controller.force_idle();
        }
    }

    fn mark_registered(&self, trigger_description: Option<String>) {
        let mut state = self.lock_state();
        if state.backend.is_some() {
            state.registration_state = ShortcutRegistrationState::Registered;
            state.registration_error = None;
            state.bound_trigger_description = trigger_description;
        }
    }

    fn mark_failed(&self, message: impl Into<String>) {
        let mut state = self.lock_state();
        state.registration_state = ShortcutRegistrationState::Failed;
        state.registration_error = Some(message.into());
        state.bound_trigger_description = None;
        state.controller.force_idle();
    }

    fn handle_phase(&self, phase: ShortcutPhase) -> ShortcutDecision {
        let mut state = self.lock_state();
        if state.registration_state != ShortcutRegistrationState::Registered {
            return ShortcutDecision::Ignore;
        }
        state.controller.handle(phase)
    }

    pub(crate) fn reset_controller(&self) {
        self.lock_state().controller.force_idle();
    }

    fn capability(&self) -> ShortcutCapabilityDto {
        let state = self.lock_state();
        shortcut_capability_for(&state)
    }

    fn lock_state(&self) -> MutexGuard<'_, ShortcutServiceState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCapabilityDto {
    platform: String,
    linux_display_server: Option<String>,
    selected_backend: Option<String>,
    registration_implemented: bool,
    registration_state: String,
    registration_error: Option<String>,
    selection_error: Option<ShortcutCapabilityErrorDto>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCapabilityErrorDto {
    code: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ShortcutDecisionEventDto {
    shortcut_id: &'static str,
    phase: &'static str,
    decision: &'static str,
}

#[tauri::command]
pub fn shortcut_capability(state: State<'_, ShortcutService>) -> ShortcutCapabilityDto {
    state.capability()
}

pub fn install_shortcut_backend<R: Runtime>(app: &mut App<R>) {
    let backend = app.state::<ShortcutService>().backend();

    match backend {
        Some(ShortcutBackend::NativeGlobalHotkey | ShortcutBackend::X11GlobalHotkey) => {
            install_native_shortcut(app);
        }
        Some(ShortcutBackend::XdgDesktopPortal) => install_portal_shortcut(app),
        None => {}
    }
}

fn install_native_shortcut<R: Runtime>(app: &mut App<R>) {
    app.state::<ShortcutService>().mark_registering();

    let builder = match tauri_plugin_global_shortcut::Builder::new()
        .with_shortcut(DEFAULT_DICTATION_TRIGGER)
    {
        Ok(builder) => builder,
        Err(error) => {
            app.state::<ShortcutService>()
                .mark_failed(format!("could not parse default global shortcut: {error}"));
            return;
        }
    };

    let plugin = builder
        .with_handler(|app, _shortcut, event| {
            let phase = match event.state {
                ShortcutState::Pressed => ShortcutPhase::Pressed,
                ShortcutState::Released => ShortcutPhase::Released,
            };
            route_shortcut_phase(app, phase);
        })
        .build();

    match app.handle().plugin(plugin) {
        Ok(()) => app
            .state::<ShortcutService>()
            .mark_registered(Some(DEFAULT_DICTATION_TRIGGER.to_owned())),
        Err(error) => app
            .state::<ShortcutService>()
            .mark_failed(format!("global shortcut registration failed: {error}")),
    }
}

#[cfg(target_os = "linux")]
fn install_portal_shortcut<R: Runtime>(app: &mut App<R>) {
    app.state::<ShortcutService>().mark_registering();
    let app_handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
        if let Err(message) = run_portal_shortcut(app_handle.clone()).await {
            app_handle.state::<ShortcutService>().mark_failed(message);
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn install_portal_shortcut<R: Runtime>(app: &mut App<R>) {
    app.state::<ShortcutService>().mark_failed(
        "XDG Desktop Portal global shortcuts are only available on Linux desktop sessions",
    );
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
enum PortalShortcutEvent {
    Phase(ShortcutPhase),
    BindingChanged(Option<String>),
}

#[cfg(target_os = "linux")]
fn trigger_for_shortcut(
    shortcuts: &[ashpd::desktop::global_shortcuts::Shortcut],
) -> Option<String> {
    shortcuts
        .iter()
        .find(|shortcut| shortcut.id() == DICTATION_SHORTCUT_ID)
        .and_then(|shortcut| {
            let trigger = shortcut.trigger_description().trim();
            (!trigger.is_empty()).then(|| trigger.to_owned())
        })
}

#[cfg(target_os = "linux")]
async fn run_portal_shortcut<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let portal = GlobalShortcuts::new()
        .await
        .map_err(|error| format!("could not connect to XDG GlobalShortcuts portal: {error}"))?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|error| format!("could not create XDG GlobalShortcuts session: {error}"))?;

    let shortcut = NewShortcut::new(DICTATION_SHORTCUT_ID, "Toggle BLCVoice dictation")
        .preferred_trigger(DEFAULT_DICTATION_TRIGGER);
    let request = portal
        .bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default())
        .await
        .map_err(|error| format!("could not request XDG global shortcut binding: {error}"))?;
    let response = request
        .response()
        .map_err(|error| format!("XDG global shortcut binding was not accepted: {error}"))?;

    if !response
        .shortcuts()
        .iter()
        .any(|shortcut| shortcut.id() == DICTATION_SHORTCUT_ID)
    {
        return Err("XDG portal response did not contain the BLCVoice shortcut".to_owned());
    }

    let initial_trigger = trigger_for_shortcut(response.shortcuts());

    let activated = portal
        .receive_activated()
        .await
        .map_err(|error| format!("could not subscribe to XDG shortcut activation: {error}"))?
        .filter_map(|event| async move {
            (event.shortcut_id() == DICTATION_SHORTCUT_ID)
                .then_some(PortalShortcutEvent::Phase(ShortcutPhase::Pressed))
        });
    let deactivated = portal
        .receive_deactivated()
        .await
        .map_err(|error| format!("could not subscribe to XDG shortcut deactivation: {error}"))?
        .filter_map(|event| async move {
            (event.shortcut_id() == DICTATION_SHORTCUT_ID)
                .then_some(PortalShortcutEvent::Phase(ShortcutPhase::Released))
        });
    let changed = portal
        .receive_shortcuts_changed()
        .await
        .map_err(|error| format!("could not subscribe to XDG shortcut changes: {error}"))?
        .map(|event| PortalShortcutEvent::BindingChanged(trigger_for_shortcut(event.shortcuts())));

    if let Some(trigger) = initial_trigger {
        app.state::<ShortcutService>()
            .mark_registered(Some(trigger));
    } else if portal.version() >= 2 {
        app.state::<ShortcutService>().mark_configuration_required();
        portal
            .configure_shortcuts(&session, None, ConfigureShortcutsOptions::default())
            .await
            .map_err(|error| format!("could not open XDG shortcut configuration: {error}"))?;
    } else {
        return Err(format!(
            "the desktop portal returned the BLCVoice shortcut without a trigger and GlobalShortcuts v{} cannot open the configuration UI",
            portal.version()
        ));
    }

    let phase_events = stream::select(activated, deactivated);
    let events = stream::select(phase_events, changed);
    futures_util::pin_mut!(events);
    while let Some(event) = events.next().await {
        match event {
            PortalShortcutEvent::Phase(phase) => route_shortcut_phase(&app, phase),
            PortalShortcutEvent::BindingChanged(Some(trigger)) => app
                .state::<ShortcutService>()
                .mark_registered(Some(trigger)),
            PortalShortcutEvent::BindingChanged(None) => {
                app.state::<ShortcutService>().mark_configuration_required();
            }
        }
    }

    let _ = session.close().await;
    Err("XDG global shortcut signal stream ended unexpectedly".to_owned())
}

fn route_shortcut_phase<R: Runtime>(app: &AppHandle<R>, phase: ShortcutPhase) {
    let decision = app.state::<ShortcutService>().handle_phase(phase);
    if decision == ShortcutDecision::Ignore {
        return;
    }

    let payload = ShortcutDecisionEventDto {
        shortcut_id: DICTATION_SHORTCUT_ID,
        phase: shortcut_phase_name(phase),
        decision: shortcut_decision_name(decision),
    };
    let _ = app.emit(SHORTCUT_DECISION_EVENT, payload);
    app.state::<ShortcutDictationCoordinator>()
        .handle_shortcut(app.clone(), decision);
}

fn shortcut_capability_for(state: &ShortcutServiceState) -> ShortcutCapabilityDto {
    let platform = state.environment.platform();
    let linux_display_server = match platform {
        DesktopPlatform::Linux => Some(state.environment.linux_display_server().to_string()),
        DesktopPlatform::Windows | DesktopPlatform::MacOs | DesktopPlatform::Other => None,
    };
    let registration_state = match state.registration_state {
        ShortcutRegistrationState::ConfigurationRequired => {
            "configuration required · KDE shortcut settings opened".to_owned()
        }
        ShortcutRegistrationState::Registered => match state.bound_trigger_description.as_deref() {
            Some(trigger) => format!("registered · {trigger}"),
            None => "registered".to_owned(),
        },
        other => other.as_str().to_owned(),
    };

    ShortcutCapabilityDto {
        platform: platform.to_string(),
        linux_display_server,
        selected_backend: state.backend.map(|backend| backend.to_string()),
        registration_implemented: state.backend.is_some(),
        registration_state,
        registration_error: state.registration_error.clone(),
        selection_error: state
            .selection_error
            .map(|error| ShortcutCapabilityErrorDto {
                code: shortcut_capability_error_code(error),
                message: error.to_string(),
            }),
    }
}

const fn shortcut_capability_error_code(error: ShortcutCapabilityError) -> &'static str {
    match error {
        ShortcutCapabilityError::UnsupportedPlatform => "unsupported_platform",
        ShortcutCapabilityError::UnknownLinuxDisplayServer => "unknown_linux_display_server",
    }
}

const fn shortcut_phase_name(phase: ShortcutPhase) -> &'static str {
    match phase {
        ShortcutPhase::Pressed => "pressed",
        ShortcutPhase::Released => "released",
    }
}

const fn shortcut_decision_name(decision: ShortcutDecision) -> &'static str {
    match decision {
        ShortcutDecision::StartDictation => "startDictation",
        ShortcutDecision::StopDictation => "stopDictation",
        ShortcutDecision::Ignore => "ignore",
    }
}

#[cfg(test)]
mod tests {
    use blcvoice_shortcuts::LinuxDisplayServer;

    use super::*;

    #[test]
    fn wayland_selects_portal_and_reports_pending_registration() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ));
        let capability = service.capability();

        assert_eq!(capability.platform, "linux");
        assert_eq!(capability.linux_display_server.as_deref(), Some("wayland"));
        assert_eq!(
            capability.selected_backend.as_deref(),
            Some("xdgDesktopPortal")
        );
        assert!(capability.registration_implemented);
        assert_eq!(capability.registration_state, "pending");
        assert_eq!(capability.registration_error, None);
        assert_eq!(capability.selection_error, None);
    }

    #[test]
    fn unknown_linux_session_is_unavailable_with_typed_error() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Unknown,
        ));
        let capability = service.capability();

        assert_eq!(capability.selected_backend, None);
        assert!(!capability.registration_implemented);
        assert_eq!(capability.registration_state, "unavailable");
        let error = capability.selection_error.expect("selection should fail");
        assert_eq!(error.code, "unknown_linux_display_server");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn non_linux_desktop_selects_native_backend() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Windows,
            LinuxDisplayServer::Unknown,
        ));
        let capability = service.capability();

        assert_eq!(capability.platform, "windows");
        assert_eq!(capability.linux_display_server, None);
        assert_eq!(
            capability.selected_backend.as_deref(),
            Some("nativeGlobalHotkey")
        );
        assert_eq!(capability.registration_state, "pending");
        assert_eq!(capability.selection_error, None);
    }

    #[test]
    fn shortcut_controller_only_routes_after_registration() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::X11,
        ));

        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::Ignore
        );

        service.mark_registered(Some("Ctrl+Shift+Space".to_owned()));
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StopDictation
        );
    }

    #[test]
    fn registration_failure_is_actionable_and_resets_controller() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Windows,
            LinuxDisplayServer::Unknown,
        ));
        service.mark_registered(Some(DEFAULT_DICTATION_TRIGGER.to_owned()));
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );

        service.mark_failed("shortcut already owned by another application");
        let capability = service.capability();
        assert_eq!(capability.registration_state, "failed");
        assert_eq!(
            capability.registration_error.as_deref(),
            Some("shortcut already owned by another application")
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );
    }

    #[test]
    fn registered_capability_reports_the_actual_trigger_description() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ));
        service.mark_registered(Some("Ctrl+Alt+Space".to_owned()));

        assert_eq!(
            service.capability().registration_state,
            "registered · Ctrl+Alt+Space"
        );
    }

    #[test]
    fn missing_portal_trigger_is_not_reported_as_registered() {
        let service = ShortcutService::for_environment(ShortcutEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ));
        service.mark_configuration_required();

        assert_eq!(
            service.capability().registration_state,
            "configuration required · KDE shortcut settings opened"
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::Ignore
        );
    }
}
