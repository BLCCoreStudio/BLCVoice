from pathlib import Path


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    source = path.read_text()
    if old not in source:
        raise SystemExit(f"{label} marker not found in {path}")
    path.write_text(source.replace(old, new, 1))


coordinator = Path("apps/desktop/src-tauri/src/coordinator.rs")
replace_once(
    coordinator,
    '''    fn reset(&self) {
        *self.lock_state() = CoordinatorState::Idle;
    }

    fn lock_state(&self) -> MutexGuard<'_, CoordinatorState> {
''',
    '''    fn reset(&self) {
        *self.lock_state() = CoordinatorState::Idle;
    }

    pub(crate) fn status(&self) -> (&'static str, Option<SessionId>) {
        match *self.lock_state() {
            CoordinatorState::Idle => ("idle", None),
            CoordinatorState::Starting { .. } => ("starting", None),
            CoordinatorState::Recording(session_id) => ("recording", Some(session_id)),
            CoordinatorState::Finishing(session_id) => ("finishing", Some(session_id)),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, CoordinatorState> {
''',
    "coordinator status",
)
replace_once(
    coordinator,
    '''    #[test]
    fn matching_finish_returns_coordinator_to_idle() {
''',
    '''    #[test]
    fn status_reports_shortcut_ownership_without_exposing_mutable_state() {
        let coordinator = ShortcutDictationCoordinator::default();
        assert_eq!(coordinator.status(), ("idle", None));

        *coordinator.lock_state() = CoordinatorState::Starting {
            stop_requested: false,
        };
        assert_eq!(coordinator.status(), ("starting", None));

        let session_id = SessionId::new(7);
        *coordinator.lock_state() = CoordinatorState::Recording(session_id);
        assert_eq!(coordinator.status(), ("recording", Some(session_id)));

        *coordinator.lock_state() = CoordinatorState::Finishing(session_id);
        assert_eq!(coordinator.status(), ("finishing", Some(session_id)));
    }

    #[test]
    fn matching_finish_returns_coordinator_to_idle() {
''',
    "coordinator status test",
)

ipc = Path("apps/desktop/src-tauri/src/ipc.rs")
replace_once(
    ipc,
    '''use crate::capture::{
    DesktopCaptureError, DesktopCaptureErrorKind, DesktopCaptureService, MicrophoneTestReport,
    session_state_name,
};
''',
    '''use crate::capture::{
    DesktopCaptureError, DesktopCaptureErrorKind, DesktopCaptureService, MicrophoneTestReport,
    session_state_name,
};
use crate::coordinator::ShortcutDictationCoordinator;
''',
    "coordinator import",
)
replace_once(
    ipc,
    '''pub struct DesktopStatusDto {
    session: Option<SessionDto>,
    last_pump_failure: Option<String>,
    dictation_state: &'static str,
    dictation_session_id: Option<u64>,
    insertion: InsertionCapabilityDto,
}
''',
    '''pub struct DesktopStatusDto {
    session: Option<SessionDto>,
    last_pump_failure: Option<String>,
    dictation_state: &'static str,
    dictation_session_id: Option<u64>,
    shortcut_dictation_state: &'static str,
    shortcut_dictation_session_id: Option<u64>,
    insertion: InsertionCapabilityDto,
}
''',
    "desktop status dto",
)
replace_once(
    ipc,
    '''#[tauri::command]
pub fn desktop_status(state: State<'_, DesktopState>) -> DesktopStatusDto {
    DesktopStatusDto {
        session: state.capture.current_session().map(SessionDto::from),
        last_pump_failure: state.capture.last_pump_failure(),
        dictation_state: state.dictation.state_name(),
        dictation_session_id: state.dictation.active_session_id().map(SessionId::get),
        insertion: InsertionCapabilityDto::from_service(&state.insertion),
    }
}
''',
    '''#[tauri::command]
pub fn desktop_status(
    state: State<'_, DesktopState>,
    shortcut: State<'_, ShortcutDictationCoordinator>,
) -> DesktopStatusDto {
    let (shortcut_dictation_state, shortcut_dictation_session_id) = shortcut.status();
    DesktopStatusDto {
        session: state.capture.current_session().map(SessionDto::from),
        last_pump_failure: state.capture.last_pump_failure(),
        dictation_state: state.dictation.state_name(),
        dictation_session_id: state.dictation.active_session_id().map(SessionId::get),
        shortcut_dictation_state,
        shortcut_dictation_session_id: shortcut_dictation_session_id.map(SessionId::get),
        insertion: InsertionCapabilityDto::from_service(&state.insertion),
    }
}
''',
    "desktop status command",
)

app = Path("apps/desktop/ui/app.js")
replace_once(
    app,
    '''const AUTO_FINISH_MS = 10_000;
const invoke = window.__TAURI__?.core?.invoke;
''',
    '''const AUTO_FINISH_MS = 10_000;
const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen;
''',
    "tauri event bridge",
)
replace_once(
    app,
    '''  dictationSessionId: null,
  dictationBusy: false,
''',
    '''  dictationSessionId: null,
  dictationBusy: false,
  shortcutDictationState: "idle",
  shortcutDictationSessionId: null,
''',
    "shortcut state",
)
replace_once(
    app,
    '''function dictationReady() {
  return Boolean(invoke && selectedDevice() && installedModelAvailable() && !state.testSessionId);
}

function updateDictationControls() {
  const active = state.dictationSessionId !== null;
  elements.dictationToggle.disabled = state.dictationBusy || (!active && !dictationReady());
  elements.dictationToggle.classList.toggle("recording", active);
  elements.dictationButtonLabel.textContent = active ? "Stop & type" : "Start dictation";
  elements.dictationCancel.hidden = !active;
  elements.dictationCancel.disabled = state.dictationBusy;
  elements.startTest.disabled = state.testBusy || Boolean(state.testSessionId) || active || !selectedDevice();
}
''',
    '''function shortcutDictationActive() {
  return state.shortcutDictationState !== "idle";
}

function anyDictationActive() {
  return state.dictationSessionId !== null || shortcutDictationActive();
}

function dictationReady() {
  return Boolean(
    invoke &&
      selectedDevice() &&
      installedModelAvailable() &&
      !state.testSessionId &&
      !shortcutDictationActive(),
  );
}

function updateDictationControls() {
  const localActive = state.dictationSessionId !== null;
  const shortcutActive = shortcutDictationActive();
  elements.dictationToggle.disabled =
    state.dictationBusy || shortcutActive || (!localActive && !dictationReady());
  elements.dictationToggle.classList.toggle(
    "recording",
    localActive || state.shortcutDictationState === "recording",
  );
  elements.dictationButtonLabel.textContent = shortcutActive
    ? "Shortcut dictation active"
    : localActive
      ? "Stop & type"
      : "Start dictation";
  elements.dictationCancel.hidden = !localActive;
  elements.dictationCancel.disabled = state.dictationBusy;
  elements.startTest.disabled =
    state.testBusy || Boolean(state.testSessionId) || anyDictationActive() || !selectedDevice();
}
''',
    "dictation ownership controls",
)
replace_once(
    app,
    '''async function refreshDevices() {
  if (!invoke || state.settingsBusy || state.testSessionId || state.dictationSessionId) return;
''',
    '''async function refreshDevices() {
  if (!invoke || state.settingsBusy || state.testSessionId || anyDictationActive()) return;
''',
    "refresh device guard",
)
replace_once(
    app,
    '''    const busy = state.modelBusyId !== null;
''',
    '''    const busy = state.modelBusyId !== null || anyDictationActive();
''',
    "model action guard",
)
replace_once(
    app,
    '''async function installModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
''',
    '''async function installModel(modelId) {
  if (!invoke || state.modelBusyId !== null || anyDictationActive()) return;
''',
    "install model guard",
)
replace_once(
    app,
    '''async function selectModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
''',
    '''async function selectModel(modelId) {
  if (!invoke || state.modelBusyId !== null || anyDictationActive()) return;
''',
    "select model guard",
)
replace_once(
    app,
    '''async function removeModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
''',
    '''async function removeModel(modelId) {
  if (!invoke || state.modelBusyId !== null || anyDictationActive()) return;
''',
    "remove model guard",
)
replace_once(
    app,
    '''async function startTest() {
  const device = selectedDevice();
  if (!invoke || !device || state.testBusy || state.testSessionId || state.dictationSessionId) return;
''',
    '''async function startTest() {
  const device = selectedDevice();
  if (!invoke || !device || state.testBusy || state.testSessionId || anyDictationActive()) return;
''',
    "microphone test ownership guard",
)

lifecycle_code = '''function shortcutLifecycleError(payload) {
  return {
    code: payload?.errorCode || "shortcut_dictation_failed",
    message: payload?.message || "Shortcut dictation failed.",
    recoverableText: payload?.recoverableText || null,
  };
}

function renderShortcutLifecycle(payload) {
  if (!payload || payload.source !== "shortcut" || state.dictationSessionId !== null) return;

  switch (payload.state) {
    case "starting":
      state.shortcutDictationState = "starting";
      state.shortcutDictationSessionId = null;
      clearDictationError();
      setPill(elements.dictationState, "Starting", "working");
      elements.dictationMessage.textContent =
        "Global shortcut is preparing the configured local recognizer…";
      break;
    case "recording":
      state.shortcutDictationState = "recording";
      state.shortcutDictationSessionId = payload.sessionId ?? null;
      clearDictationError();
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent =
        "Shortcut dictation is listening locally. Press Ctrl + Shift + Space again to stop.";
      break;
    case "finishing":
      state.shortcutDictationState = "finishing";
      state.shortcutDictationSessionId = payload.sessionId ?? state.shortcutDictationSessionId;
      setPill(elements.dictationState, "Transcribing", "working");
      elements.dictationMessage.textContent =
        "Finalizing audio, transcribing locally and submitting the transcript…";
      break;
    case "completed": {
      state.shortcutDictationState = "idle";
      state.shortcutDictationSessionId = null;
      const backend = payload.insertionBackend || "system insertion";
      if (payload.text) showTranscript(payload.text, `Shortcut · ${backend}`);
      clearDictationError();
      setPill(elements.dictationState, "Ready", "passed");
      elements.dictationMessage.textContent = `Shortcut dictation completed through ${backend}.`;
      break;
    }
    case "failed":
      state.shortcutDictationState = "idle";
      state.shortcutDictationSessionId = null;
      setPill(elements.dictationState, "Failed", "failed");
      elements.dictationMessage.textContent = "Shortcut dictation stopped before a clean completion.";
      showDictationError(shortcutLifecycleError(payload));
      break;
    default:
      return;
  }
  renderModels();
  updateDictationControls();
}

async function subscribeShortcutLifecycle() {
  if (!listen) return;
  await listen("blcvoice://dictation-lifecycle", (event) => renderShortcutLifecycle(event.payload));
}

'''
replace_once(
    app,
    '''async function recoverDesktopStatus() {
''',
    lifecycle_code + '''async function recoverDesktopStatus() {
''',
    "shortcut lifecycle listener",
)
replace_once(
    app,
    '''async function recoverDesktopStatus() {
  try {
    const status = await invoke("desktop_status");
    if (status.dictationSessionId && status.dictationState === "recording") {
      state.dictationSessionId = status.dictationSessionId;
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent = "A dictation session was already active.";
    } else if (status.dictationState === "idle") {
      setPill(elements.dictationState, "Ready", "idle");
      elements.dictationMessage.textContent = "Press Start dictation or use Ctrl + Shift + Space.";
    }
  } catch (error) {
    showDictationError(error);
  }
}
''',
    '''async function recoverDesktopStatus() {
  try {
    const status = await invoke("desktop_status");
    state.shortcutDictationState = status.shortcutDictationState || "idle";
    state.shortcutDictationSessionId = status.shortcutDictationSessionId ?? null;

    if (state.shortcutDictationState === "starting") {
      setPill(elements.dictationState, "Starting", "working");
      elements.dictationMessage.textContent = "A shortcut-owned dictation is preparing.";
    } else if (state.shortcutDictationState === "recording") {
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent =
        "A shortcut-owned dictation is active. Press Ctrl + Shift + Space again to stop.";
    } else if (state.shortcutDictationState === "finishing") {
      setPill(elements.dictationState, "Transcribing", "working");
      elements.dictationMessage.textContent = "A shortcut-owned dictation is finishing.";
    } else if (status.dictationSessionId && status.dictationState === "recording") {
      state.dictationSessionId = status.dictationSessionId;
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent = "A button-owned dictation session was already active.";
    } else if (status.dictationState === "idle") {
      setPill(elements.dictationState, "Ready", "idle");
      elements.dictationMessage.textContent = "Press Start dictation or use Ctrl + Shift + Space.";
    }
  } catch (error) {
    showDictationError(error);
  }
}
''',
    "desktop status ownership recovery",
)
replace_once(
    app,
    '''  try {
    await loadSettings();
    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);
''',
    '''  try {
    await subscribeShortcutLifecycle();
    await loadSettings();
    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);
''',
    "bootstrap lifecycle subscription",
)

adr = Path("docs/adr/0024-shortcut-dictation-coordinator.md")
source = adr.read_text()
needle = "- The UI becomes a projection of backend state rather than an orchestration dependency.\n"
replacement = needle + "- Desktop status exposes shortcut coordinator ownership so a reloaded webview can recover state without guessing whether it owns the active session.\n"
if needle not in source:
    raise SystemExit("ADR consequence marker not found")
adr.write_text(source.replace(needle, replacement, 1))
