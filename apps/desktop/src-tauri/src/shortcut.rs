use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(target_os = "linux")]
use blcvoice_shortcuts::{DEFAULT_DICTATION_TRIGGER, DICTATION_SHORTCUT_ID};
use blcvoice_shortcuts::{
    DictationShortcutMode, ShortcutController, ShortcutDecision, ShortcutModeError, ShortcutPhase,
};
use serde::Serialize;
use tauri::{App, AppHandle, Emitter, State};

pub const SHORTCUT_DECISION_EVENT: &str = "blcvoice://shortcut-decision";
const NATIVE_TRIGGER_DESCRIPTION: &str = "Ctrl+Shift+Space";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutBackend {
    Pending,
    NativeGlobalHotkey,
    XdgPortal,
}

impl ShortcutBackend {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::NativeGlobalHotkey => "nativeGlobalHotkey",
            Self::XdgPortal => "xdgPortal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutRegistrationState {
    Initializing,
    Registered,
    Failed,
}

impl ShortcutRegistrationState {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Registered => "registered",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRuntimeStatus {
    pub backend: ShortcutBackend,
    pub registration: ShortcutRegistrationState,
    pub mode: DictationShortcutMode,
    pub trigger_description: String,
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct ShortcutInner {
    controller: ShortcutController,
    backend: ShortcutBackend,
    registration: ShortcutRegistrationState,
    trigger_description: String,
    last_error: Option<String>,
}

#[derive(Debug)]
pub struct DesktopShortcutService {
    inner: Mutex<ShortcutInner>,
}

impl Default for DesktopShortcutService {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopShortcutService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ShortcutInner {
                controller: ShortcutController::default(),
                backend: ShortcutBackend::Pending,
                registration: ShortcutRegistrationState::Initializing,
                trigger_description: NATIVE_TRIGGER_DESCRIPTION.to_owned(),
                last_error: None,
            }),
        }
    }

    #[must_use]
    pub fn status(&self) -> ShortcutRuntimeStatus {
        let inner = self.lock_inner();
        ShortcutRuntimeStatus {
            backend: inner.backend,
            registration: inner.registration,
            mode: inner.controller.mode(),
            trigger_description: inner.trigger_description.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    pub fn set_mode(&self, mode: DictationShortcutMode) -> Result<(), ShortcutModeError> {
        self.lock_inner().controller.set_mode(mode)
    }

    #[must_use]
    pub fn handle_phase(&self, phase: ShortcutPhase) -> ShortcutDecision {
        self.lock_inner().controller.handle(phase)
    }

    fn mark_initializing(&self, backend: ShortcutBackend, trigger: impl Into<String>) {
        let mut inner = self.lock_inner();
        inner.backend = backend;
        inner.registration = ShortcutRegistrationState::Initializing;
        inner.trigger_description = trigger.into();
        inner.last_error = None;
    }

    fn mark_registered(&self, backend: ShortcutBackend, trigger: impl Into<String>) {
        let mut inner = self.lock_inner();
        inner.backend = backend;
        inner.registration = ShortcutRegistrationState::Registered;
        inner.trigger_description = trigger.into();
        inner.last_error = None;
    }

    fn mark_failed(&self, backend: ShortcutBackend, message: impl Into<String>) {
        let mut inner = self.lock_inner();
        inner.backend = backend;
        inner.registration = ShortcutRegistrationState::Failed;
        inner.controller.force_idle();
        inner.last_error = Some(message.into());
    }

    fn lock_inner(&self) -> MutexGuard<'_, ShortcutInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug)]
pub struct ShortcutState {
    service: Arc<DesktopShortcutService>,
}

impl ShortcutState {
    #[must_use]
    pub fn production() -> Self {
        Self {
            service: Arc::new(DesktopShortcutService::new()),
        }
    }

    #[must_use]
    pub fn service(&self) -> Arc<DesktopShortcutService> {
        Arc::clone(&self.service)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutDecisionEvent {
    action: &'static str,
    mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatusDto {
    backend: &'static str,
    registration: &'static str,
    mode: String,
    trigger_description: String,
    last_error: Option<String>,
}

impl From<ShortcutRuntimeStatus> for ShortcutStatusDto {
    fn from(status: ShortcutRuntimeStatus) -> Self {
        Self {
            backend: status.backend.name(),
            registration: status.registration.name(),
            mode: status.mode.to_string(),
            trigger_description: status.trigger_description,
            last_error: status.last_error,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCommandErrorDto {
    code: &'static str,
    message: String,
}

#[tauri::command]
pub fn global_shortcut_status(state: State<'_, ShortcutState>) -> ShortcutStatusDto {
    ShortcutStatusDto::from(state.service.status())
}

#[tauri::command]
pub fn global_shortcut_set_mode(
    state: State<'_, ShortcutState>,
    mode: String,
) -> Result<ShortcutStatusDto, ShortcutCommandErrorDto> {
    let requested_mode = match mode.as_str() {
        "toggle" => DictationShortcutMode::Toggle,
        "pushToTalk" | "push_to_talk" => DictationShortcutMode::PushToTalk,
        _ => {
            return Err(ShortcutCommandErrorDto {
                code: "invalid_shortcut_mode",
                message: format!("unsupported dictation shortcut mode: {mode}"),
            });
        }
    };

    state
        .service
        .set_mode(requested_mode)
        .map_err(|error| ShortcutCommandErrorDto {
            code: "shortcut_busy",
            message: error.to_string(),
        })?;

    Ok(ShortcutStatusDto::from(state.service.status()))
}

pub fn install_global_shortcut(app: &mut App, service: Arc<DesktopShortcutService>) {
    #[cfg(target_os = "linux")]
    if linux_backend_choice(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    ) == LinuxBackendChoice::XdgPortal
    {
        install_wayland_portal(app, service);
        return;
    }

    install_native_global_hotkey(app, service);
}

fn install_native_global_hotkey(app: &mut App, service: Arc<DesktopShortcutService>) {
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    service.mark_initializing(
        ShortcutBackend::NativeGlobalHotkey,
        NATIVE_TRIGGER_DESCRIPTION,
    );

    let native_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    let callback_shortcut = native_shortcut;
    let callback_service = Arc::clone(&service);

    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |app_handle, shortcut, event| {
            if shortcut != &callback_shortcut {
                return;
            }
            let phase = match event.state() {
                ShortcutState::Pressed => ShortcutPhase::Pressed,
                ShortcutState::Released => ShortcutPhase::Released,
            };
            dispatch_phase(app_handle, &callback_service, phase);
        })
        .build();

    if let Err(error) = app.handle().plugin(plugin) {
        service.mark_failed(
            ShortcutBackend::NativeGlobalHotkey,
            format!("could not initialize native global shortcut plugin: {error}"),
        );
        return;
    }

    if let Err(error) = app.global_shortcut().register(native_shortcut) {
        service.mark_failed(
            ShortcutBackend::NativeGlobalHotkey,
            format!("could not register {NATIVE_TRIGGER_DESCRIPTION}: {error}"),
        );
        return;
    }

    service.mark_registered(
        ShortcutBackend::NativeGlobalHotkey,
        NATIVE_TRIGGER_DESCRIPTION,
    );
}

fn dispatch_phase(app_handle: &AppHandle, service: &DesktopShortcutService, phase: ShortcutPhase) {
    let decision = service.handle_phase(phase);
    let action = match decision {
        ShortcutDecision::StartDictation => "startDictation",
        ShortcutDecision::StopDictation => "stopDictation",
        ShortcutDecision::Ignore => return,
    };
    let mode = service.status().mode.to_string();
    let _ = app_handle.emit(
        SHORTCUT_DECISION_EVENT,
        ShortcutDecisionEvent { action, mode },
    );
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinuxBackendChoice {
    NativeGlobalHotkey,
    XdgPortal,
}

#[cfg(target_os = "linux")]
fn linux_backend_choice(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
) -> LinuxBackendChoice {
    let reports_wayland =
        xdg_session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"));
    let has_wayland_display = wayland_display.is_some_and(|value| !value.trim().is_empty());

    if reports_wayland || has_wayland_display {
        LinuxBackendChoice::XdgPortal
    } else {
        LinuxBackendChoice::NativeGlobalHotkey
    }
}

#[cfg(target_os = "linux")]
fn install_wayland_portal(app: &App, service: Arc<DesktopShortcutService>) {
    service.mark_initializing(ShortcutBackend::XdgPortal, DEFAULT_DICTATION_TRIGGER);
    let app_handle = app.handle().clone();
    let task_service = Arc::clone(&service);

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_wayland_portal(app_handle, Arc::clone(&task_service)).await {
            task_service.mark_failed(ShortcutBackend::XdgPortal, error);
        }
    });
}

#[cfg(target_os = "linux")]
async fn run_wayland_portal(
    app_handle: AppHandle,
    service: Arc<DesktopShortcutService>,
) -> Result<(), String> {
    use ashpd::desktop::{
        CreateSessionOptions,
        global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut},
    };
    use futures_util::{FutureExt, StreamExt};

    let portal = GlobalShortcuts::new()
        .await
        .map_err(|error| format!("XDG GlobalShortcuts portal is unavailable: {error}"))?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|error| format!("could not create XDG shortcut session: {error}"))?;

    let activated = portal
        .receive_activated()
        .await
        .map_err(|error| format!("could not subscribe to shortcut activation: {error}"))?;
    let deactivated = portal
        .receive_deactivated()
        .await
        .map_err(|error| format!("could not subscribe to shortcut deactivation: {error}"))?;

    let requested = [
        NewShortcut::new(DICTATION_SHORTCUT_ID, "Start or stop BLCVoice dictation")
            .preferred_trigger(Some(DEFAULT_DICTATION_TRIGGER)),
    ];
    let request = portal
        .bind_shortcuts(&session, &requested, None, BindShortcutsOptions::default())
        .await
        .map_err(|error| format!("could not request XDG shortcut binding: {error}"))?;
    let response = request
        .response()
        .map_err(|error| format!("XDG shortcut binding was not accepted: {error}"))?;

    let Some(bound) = response
        .shortcuts()
        .iter()
        .find(|shortcut| shortcut.id() == DICTATION_SHORTCUT_ID)
    else {
        let _ = session.close().await;
        return Err("XDG portal did not return the BLCVoice dictation shortcut".to_owned());
    };

    service.mark_registered(ShortcutBackend::XdgPortal, bound.trigger_description());

    futures_util::pin_mut!(activated);
    futures_util::pin_mut!(deactivated);

    let result = loop {
        futures_util::select! {
            event = activated.next().fuse() => match event {
                Some(event) if event.shortcut_id() == DICTATION_SHORTCUT_ID => {
                    dispatch_phase(&app_handle, &service, ShortcutPhase::Pressed);
                }
                Some(_) => {}
                None => break Err("XDG shortcut activation stream ended".to_owned()),
            },
            event = deactivated.next().fuse() => match event {
                Some(event) if event.shortcut_id() == DICTATION_SHORTCUT_ID => {
                    dispatch_phase(&app_handle, &service, ShortcutPhase::Released);
                }
                Some(_) => {}
                None => break Err("XDG shortcut deactivation stream ended".to_owned()),
            },
        }
    };

    let _ = session.close().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_uses_toggle_semantics_by_default() {
        let service = DesktopShortcutService::new();

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
    fn failed_backend_forces_requested_dictation_idle() {
        let service = DesktopShortcutService::new();
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );

        service.mark_failed(ShortcutBackend::NativeGlobalHotkey, "test failure");

        let status = service.status();
        assert_eq!(status.registration, ShortcutRegistrationState::Failed);
        assert_eq!(status.last_error.as_deref(), Some("test failure"));
    }

    #[test]
    fn mode_can_be_configured_while_idle() {
        let service = DesktopShortcutService::new();
        service
            .set_mode(DictationShortcutMode::PushToTalk)
            .expect("idle shortcut mode should be configurable");

        assert_eq!(service.status().mode, DictationShortcutMode::PushToTalk);
        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Released),
            ShortcutDecision::StopDictation
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_environment_selects_portal() {
        assert_eq!(
            linux_backend_choice(Some("wayland"), Some("wayland-0")),
            LinuxBackendChoice::XdgPortal
        );
        assert_eq!(
            linux_backend_choice(Some("x11"), Some("wayland-1")),
            LinuxBackendChoice::XdgPortal
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn x11_environment_selects_native_backend() {
        assert_eq!(
            linux_backend_choice(Some("x11"), None),
            LinuxBackendChoice::NativeGlobalHotkey
        );
        assert_eq!(
            linux_backend_choice(None, None),
            LinuxBackendChoice::NativeGlobalHotkey
        );
    }
}
