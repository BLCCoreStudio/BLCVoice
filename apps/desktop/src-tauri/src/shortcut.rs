use std::sync::{Arc, Mutex, MutexGuard};

use blcvoice_shortcuts::{
    DEFAULT_DICTATION_TRIGGER, DICTATION_SHORTCUT_ID, DictationShortcutMode, ShortcutController,
    ShortcutDecision, ShortcutPhase,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutBackendKind {
    NativeGlobalHotkey,
    XdgGlobalShortcutsPortal,
}

impl ShortcutBackendKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NativeGlobalHotkey => "nativeGlobalHotkey",
            Self::XdgGlobalShortcutsPortal => "xdgGlobalShortcutsPortal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutRegistrationState {
    Pending,
    Registered,
    Failed,
}

impl ShortcutRegistrationState {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Registered => "registered",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutStatusSnapshot {
    pub backend: ShortcutBackendKind,
    pub registration: ShortcutRegistrationState,
    pub preferred_trigger: String,
    pub trigger_description: Option<String>,
    pub mode: DictationShortcutMode,
    pub activations: u64,
    pub deactivations: u64,
    pub last_decision: Option<ShortcutDecision>,
    pub last_error: Option<String>,
}

#[derive(Debug)]
struct ShortcutRuntimeState {
    controller: ShortcutController,
    backend: ShortcutBackendKind,
    registration: ShortcutRegistrationState,
    trigger_description: Option<String>,
    activations: u64,
    deactivations: u64,
    last_decision: Option<ShortcutDecision>,
    last_error: Option<String>,
}

#[derive(Debug)]
pub struct DesktopShortcutService {
    state: Mutex<ShortcutRuntimeState>,
}

impl DesktopShortcutService {
    #[must_use]
    pub fn production() -> Self {
        Self::new(production_backend())
    }

    #[must_use]
    pub fn new(backend: ShortcutBackendKind) -> Self {
        Self {
            state: Mutex::new(ShortcutRuntimeState {
                controller: ShortcutController::default(),
                backend,
                registration: ShortcutRegistrationState::Pending,
                trigger_description: None,
                activations: 0,
                deactivations: 0,
                last_decision: None,
                last_error: None,
            }),
        }
    }

    #[must_use]
    pub fn backend(&self) -> ShortcutBackendKind {
        self.lock_state().backend
    }

    #[must_use]
    pub fn status(&self) -> ShortcutStatusSnapshot {
        let state = self.lock_state();
        ShortcutStatusSnapshot {
            backend: state.backend,
            registration: state.registration,
            preferred_trigger: DEFAULT_DICTATION_TRIGGER.to_owned(),
            trigger_description: state.trigger_description.clone(),
            mode: state.controller.mode(),
            activations: state.activations,
            deactivations: state.deactivations,
            last_decision: state.last_decision,
            last_error: state.last_error.clone(),
        }
    }

    pub fn mark_registered(&self, trigger_description: Option<String>) {
        let mut state = self.lock_state();
        state.registration = ShortcutRegistrationState::Registered;
        state.trigger_description = trigger_description;
        state.last_error = None;
    }

    pub fn mark_failed(&self, message: impl Into<String>) {
        let mut state = self.lock_state();
        state.registration = ShortcutRegistrationState::Failed;
        state.last_error = Some(message.into());
        state.controller.force_idle();
    }

    #[must_use]
    pub fn handle_phase(&self, phase: ShortcutPhase) -> ShortcutDecision {
        let mut state = self.lock_state();
        match phase {
            ShortcutPhase::Pressed => state.activations = state.activations.saturating_add(1),
            ShortcutPhase::Released => {
                state.deactivations = state.deactivations.saturating_add(1);
            }
        }
        let decision = state.controller.handle(phase);
        state.last_decision = Some(decision);
        decision
    }

    fn lock_state(&self) -> MutexGuard<'_, ShortcutRuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub fn install_backend<R: tauri::Runtime>(
    app: &mut tauri::App<R>,
    service: Arc<DesktopShortcutService>,
) {
    match service.backend() {
        ShortcutBackendKind::NativeGlobalHotkey => install_native_backend(app, service),
        ShortcutBackendKind::XdgGlobalShortcutsPortal => install_portal_backend(service),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn install_native_backend<R: tauri::Runtime>(
    app: &mut tauri::App<R>,
    service: Arc<DesktopShortcutService>,
) {
    use tauri_plugin_global_shortcut::{
        Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
    };

    fn dictation_shortcut() -> Shortcut {
        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
    }

    let expected = dictation_shortcut();
    let handler_service = Arc::clone(&service);
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, shortcut, event| {
            if shortcut != &expected {
                return;
            }
            let phase = match event.state() {
                ShortcutState::Pressed => ShortcutPhase::Pressed,
                ShortcutState::Released => ShortcutPhase::Released,
            };
            let _ = handler_service.handle_phase(phase);
        })
        .build();

    if let Err(error) = app.handle().plugin(plugin) {
        service.mark_failed(format!("could not initialize native global shortcut backend: {error}"));
        return;
    }

    if let Err(error) = app.global_shortcut().register(dictation_shortcut()) {
        service.mark_failed(format!("could not register Ctrl+Shift+Space globally: {error}"));
        return;
    }

    service.mark_registered(Some("Ctrl+Shift+Space".to_owned()));
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn install_native_backend<R: tauri::Runtime>(
    _app: &mut tauri::App<R>,
    service: Arc<DesktopShortcutService>,
) {
    service.mark_failed("native global shortcuts are not supported on this platform");
}

#[cfg(target_os = "linux")]
fn install_portal_backend(service: Arc<DesktopShortcutService>) {
    let failure_service = Arc::clone(&service);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_global_shortcuts_portal(service).await {
            failure_service.mark_failed(error);
        }
    });
}

#[cfg(not(target_os = "linux"))]
fn install_portal_backend(service: Arc<DesktopShortcutService>) {
    service.mark_failed("the XDG GlobalShortcuts portal backend is only available on Linux");
}

#[cfg(target_os = "linux")]
async fn run_global_shortcuts_portal(
    service: Arc<DesktopShortcutService>,
) -> Result<(), String> {
    use ashpd::desktop::global_shortcuts::{
        Activated, Deactivated, GlobalShortcuts, NewShortcut,
    };
    use futures_util::{StreamExt, pin_mut, stream};

    enum PortalEvent {
        Activated(Activated),
        Deactivated(Deactivated),
    }

    let portal = GlobalShortcuts::new()
        .await
        .map_err(|error| format!("could not connect to XDG GlobalShortcuts portal: {error}"))?;
    let session = portal
        .create_session(Default::default())
        .await
        .map_err(|error| format!("could not create XDG GlobalShortcuts session: {error}"))?;

    // Subscribe before binding so a compositor cannot emit the first signal in
    // the small window between a successful bind and signal subscription.
    let activated = portal
        .receive_activated()
        .await
        .map_err(|error| format!("could not subscribe to shortcut activation signals: {error}"))?
        .map(PortalEvent::Activated);
    let deactivated = portal
        .receive_deactivated()
        .await
        .map_err(|error| format!("could not subscribe to shortcut deactivation signals: {error}"))?
        .map(PortalEvent::Deactivated);

    let shortcut = NewShortcut::new(DICTATION_SHORTCUT_ID, "Start or stop BLCVoice dictation")
        .preferred_trigger(Some(DEFAULT_DICTATION_TRIGGER));
    let request = portal
        .bind_shortcuts(&session, &[shortcut], None, Default::default())
        .await
        .map_err(|error| format!("could not request the BLCVoice global shortcut: {error}"))?;
    let response = request
        .response()
        .map_err(|error| format!("the compositor rejected the BLCVoice global shortcut: {error}"))?;

    let trigger_description = response
        .shortcuts()
        .iter()
        .find(|shortcut| shortcut.id() == DICTATION_SHORTCUT_ID)
        .map(|shortcut| shortcut.trigger_description().to_owned())
        .ok_or_else(|| "the compositor returned no binding for the BLCVoice shortcut".to_owned())?;
    service.mark_registered(Some(trigger_description));

    let events = stream::select(activated, deactivated);
    pin_mut!(events);
    while let Some(event) = events.next().await {
        match event {
            PortalEvent::Activated(event)
                if event.session_handle() == session.path()
                    && event.shortcut_id() == DICTATION_SHORTCUT_ID =>
            {
                let _ = service.handle_phase(ShortcutPhase::Pressed);
            }
            PortalEvent::Deactivated(event)
                if event.session_handle() == session.path()
                    && event.shortcut_id() == DICTATION_SHORTCUT_ID =>
            {
                let _ = service.handle_phase(ShortcutPhase::Released);
            }
            PortalEvent::Activated(_) | PortalEvent::Deactivated(_) => {}
        }
    }

    Err("XDG GlobalShortcuts portal signal stream closed unexpectedly".to_owned())
}

fn production_backend() -> ShortcutBackendKind {
    #[cfg(target_os = "linux")]
    {
        let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
        let session_type = std::env::var("XDG_SESSION_TYPE").ok();
        return select_linux_backend(wayland_display.as_deref(), session_type.as_deref());
    }

    #[cfg(not(target_os = "linux"))]
    ShortcutBackendKind::NativeGlobalHotkey
}

fn select_linux_backend(
    wayland_display: Option<&str>,
    xdg_session_type: Option<&str>,
) -> ShortcutBackendKind {
    let has_wayland_display = wayland_display.is_some_and(|value| !value.trim().is_empty());
    let reports_wayland = xdg_session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"));

    if has_wayland_display || reports_wayland {
        ShortcutBackendKind::XdgGlobalShortcutsPortal
    } else {
        ShortcutBackendKind::NativeGlobalHotkey
    }
}

#[must_use]
pub const fn shortcut_decision_name(decision: ShortcutDecision) -> &'static str {
    match decision {
        ShortcutDecision::StartDictation => "startDictation",
        ShortcutDecision::StopDictation => "stopDictation",
        ShortcutDecision::Ignore => "ignore",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_display_has_priority_over_x11_presence() {
        assert_eq!(
            select_linux_backend(Some("wayland-0"), Some("x11")),
            ShortcutBackendKind::XdgGlobalShortcutsPortal
        );
    }

    #[test]
    fn session_type_detects_wayland_without_wayland_display() {
        assert_eq!(
            select_linux_backend(None, Some("Wayland")),
            ShortcutBackendKind::XdgGlobalShortcutsPortal
        );
    }

    #[test]
    fn x11_session_uses_native_backend() {
        assert_eq!(
            select_linux_backend(None, Some("x11")),
            ShortcutBackendKind::NativeGlobalHotkey
        );
    }

    #[test]
    fn missing_linux_session_hints_use_native_backend() {
        assert_eq!(
            select_linux_backend(None, None),
            ShortcutBackendKind::NativeGlobalHotkey
        );
    }

    #[test]
    fn service_records_real_phases_and_decisions() {
        let service = DesktopShortcutService::new(ShortcutBackendKind::NativeGlobalHotkey);
        service.mark_registered(Some("Ctrl+Shift+Space".to_owned()));

        assert_eq!(
            service.handle_phase(ShortcutPhase::Pressed),
            ShortcutDecision::StartDictation
        );
        assert_eq!(
            service.handle_phase(ShortcutPhase::Released),
            ShortcutDecision::Ignore
        );

        let status = service.status();
        assert_eq!(status.registration, ShortcutRegistrationState::Registered);
        assert_eq!(status.activations, 1);
        assert_eq!(status.deactivations, 1);
        assert_eq!(status.last_decision, Some(ShortcutDecision::Ignore));
    }

    #[test]
    fn registration_failure_is_diagnostic_not_a_panic() {
        let service = DesktopShortcutService::new(ShortcutBackendKind::NativeGlobalHotkey);
        service.mark_failed("shortcut already taken");

        let status = service.status();
        assert_eq!(status.registration, ShortcutRegistrationState::Failed);
        assert_eq!(status.last_error.as_deref(), Some("shortcut already taken"));
    }
}
