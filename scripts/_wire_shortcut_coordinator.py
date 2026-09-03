from pathlib import Path


def replace_section(text: str, start: str, end: str, replacement: str) -> str:
    start_index = text.find(start)
    if start_index < 0:
        raise SystemExit(f"section start not found: {start[:80]!r}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise SystemExit(f"section end not found: {end[:80]!r}")
    return text[:start_index] + replacement + text[end_index:]


ipc = Path("apps/desktop/src-tauri/src/ipc.rs")
s = ipc.read_text()

s = s.replace("use tauri::State;", "use tauri::{Manager, State};", 1)

needle = '''    fn insertion(error: InsertionError, recoverable_text: String) -> Self {
        let code = match error.kind() {
            InsertionErrorKind::InvalidText => "insertion_invalid_text",
            InsertionErrorKind::PermissionDenied => "insertion_permission_denied",
            InsertionErrorKind::BackendUnavailable => "insertion_backend_unavailable",
            InsertionErrorKind::PartialSubmission => "insertion_partial_submission",
            InsertionErrorKind::BackendFailure => "insertion_backend_failed",
        };
        Self {
            code,
            message: error.message().to_owned(),
            recoverable_text: Some(recoverable_text),
        }
    }
'''
replacement = needle + '''
    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn recoverable_text(&self) -> Option<&str> {
        self.recoverable_text.as_deref()
    }
'''
if needle not in s:
    raise SystemExit("CommandErrorDto insertion block not found")
s = s.replace(needle, replacement, 1)

needle = '''impl DictationReportDto {
    fn completed(
'''
replacement = '''impl DictationReportDto {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn insertion_backend(&self) -> &str {
        &self.insertion_backend
    }

    fn completed(
'''
if needle not in s:
    raise SystemExit("DictationReportDto impl not found")
s = s.replace(needle, replacement, 1)

marker = '''#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandErrorDto'''
helper_impl = r'''
impl DesktopState {
    pub(crate) fn start_configured_dictation(
        &self,
    ) -> Result<SessionSnapshot, CommandErrorDto> {
        let mut snapshot = self.settings.snapshot();
        let discovery = self.capture.discover_input_devices();
        let device = snapshot
            .selected_input_device_id()
            .and_then(|selected| {
                discovery
                    .devices
                    .iter()
                    .find(|device| device.id.as_str() == selected)
            })
            .or_else(|| discovery.devices.iter().find(|device| device.is_default))
            .or_else(|| discovery.devices.first())
            .ok_or_else(|| {
                CommandErrorDto::plain("no_input_device", "no usable microphone is available")
            })?;
        let device_id = device.id.clone();
        if snapshot.selected_input_device_id() != Some(device_id.as_str()) {
            snapshot = self
                .settings
                .set_input_device(Some(device_id.to_string()))
                .map_err(CommandErrorDto::from)?;
        }

        let selected_model = snapshot.selected_model_id().and_then(|id| {
            self.models
                .installed_model_path(id)
                .ok()
                .flatten()
                .map(|path| (id.to_owned(), path))
        });
        let (model_id, model_path) = if let Some(selected) = selected_model {
            selected
        } else {
            let catalog = self.models.catalog();
            let chosen = catalog
                .iter()
                .find(|status| status.installed && status.recommended)
                .or_else(|| catalog.iter().find(|status| status.installed))
                .ok_or_else(|| {
                    CommandErrorDto::plain(
                        "model_not_installed",
                        "install a speech model before starting dictation",
                    )
                })?;
            let path = self
                .models
                .installed_model_path(chosen.spec.id())
                .map_err(CommandErrorDto::from)?
                .ok_or_else(|| {
                    CommandErrorDto::plain(
                        "model_not_installed",
                        "speech model disappeared before dictation started",
                    )
                })?;
            (chosen.spec.id().to_owned(), path)
        };
        if snapshot.selected_model_id() != Some(model_id.as_str()) {
            snapshot = self
                .settings
                .set_model(Some(model_id))
                .map_err(CommandErrorDto::from)?;
        }

        let recognition = RecognitionOptions {
            language_hint: snapshot.language_hint().map(str::to_owned),
            ..RecognitionOptions::default()
        };
        self.dictation
            .start(DesktopDictationRequest {
                device_id,
                model_path,
                recognition,
            })
            .map_err(CommandErrorDto::from)
    }

    pub(crate) fn finish_dictation_session(
        &self,
        session_id: SessionId,
    ) -> Result<DictationReportDto, CommandErrorDto> {
        let report = self
            .dictation
            .finish(session_id)
            .map_err(CommandErrorDto::from)?;
        let text = report.transcription.capture.transcription.text.clone();
        self.dictation
            .begin_insertion(session_id)
            .map_err(CommandErrorDto::from)?;

        let receipt = match self.insertion.insert_text(&text) {
            Ok(receipt) => receipt,
            Err(error) => {
                let lifecycle_failure = self.dictation.fail_insertion(session_id).err();
                let mut dto = CommandErrorDto::insertion(error, text);
                if let Some(lifecycle_failure) = lifecycle_failure {
                    dto.message = format!(
                        "{}; additionally, insertion failure could not be committed to the lifecycle: {}",
                        dto.message, lifecycle_failure
                    );
                }
                return Err(dto);
            }
        };

        let completed = self
            .dictation
            .complete_insertion(session_id)
            .map_err(CommandErrorDto::from)?;
        Ok(DictationReportDto::completed(report, receipt, completed))
    }

    pub(crate) fn cancel_dictation_session(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, CommandErrorDto> {
        self.dictation
            .cancel(session_id)
            .map_err(CommandErrorDto::from)
    }
}

'''
if marker not in s:
    raise SystemExit("CommandErrorDto marker not found")
s = s.replace(marker, helper_impl + marker, 1)

finish_start = '''#[tauri::command]
pub async fn dictation_finish('''
finish_end = '''#[tauri::command]
pub async fn dictation_cancel('''
finish_replacement = '''#[tauri::command]
pub async fn dictation_finish(
    app: tauri::AppHandle,
    session_id: u64,
) -> Result<DictationReportDto, CommandErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DesktopState>()
            .finish_dictation_session(SessionId::new(session_id))
    })
    .await
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
    })?
}

'''
s = replace_section(s, finish_start, finish_end, finish_replacement)

cancel_start = '''#[tauri::command]
pub async fn dictation_cancel('''
cancel_end = '''#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusDto'''
cancel_replacement = '''#[tauri::command]
pub async fn dictation_cancel(
    app: tauri::AppHandle,
    session_id: u64,
) -> Result<SessionDto, CommandErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DesktopState>()
            .cancel_dictation_session(SessionId::new(session_id))
            .map(SessionDto::from)
    })
    .await
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
    })?
}

'''
s = replace_section(s, cancel_start, cancel_end, cancel_replacement)

configured_start = '''#[tauri::command]
pub async fn dictation_start_configured('''
configured_end = '''#[tauri::command]
pub fn insertion_capability'''
configured_replacement = '''#[tauri::command]
pub async fn dictation_start_configured(
    app: tauri::AppHandle,
) -> Result<SessionDto, CommandErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<DesktopState>()
            .start_configured_dictation()
            .map(SessionDto::from)
    })
    .await
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("dictation worker failed: {error}"))
    })?
}

'''
s = replace_section(s, configured_start, configured_end, configured_replacement)

ipc.write_text(s)

shortcut = Path("apps/desktop/src-tauri/src/shortcut.rs")
s = shortcut.read_text()
if "use crate::coordinator::ShortcutDictationCoordinator;" not in s:
    s = s.replace(
        "use tauri::{App, AppHandle, Emitter, Manager, Runtime, State};\n",
        "use tauri::{App, AppHandle, Emitter, Manager, Runtime, State};\n\nuse crate::coordinator::ShortcutDictationCoordinator;\n",
        1,
    )
needle = '''    fn handle_phase(&self, phase: ShortcutPhase) -> ShortcutDecision {
        let mut state = self.lock_state();
        if state.registration_state != ShortcutRegistrationState::Registered {
            return ShortcutDecision::Ignore;
        }
        state.controller.handle(phase)
    }
'''
replacement = needle + '''
    pub(crate) fn reset_controller(&self) {
        self.lock_state().controller.force_idle();
    }
'''
if needle not in s:
    raise SystemExit("ShortcutService handle_phase block not found")
s = s.replace(needle, replacement, 1)
needle = '''    let _ = app.emit(SHORTCUT_DECISION_EVENT, payload);
}
'''
replacement = '''    let _ = app.emit(SHORTCUT_DECISION_EVENT, payload);
    app.state::<ShortcutDictationCoordinator>()
        .handle_shortcut(app.clone(), decision);
}
'''
if needle not in s:
    raise SystemExit("shortcut event routing block not found")
s = s.replace(needle, replacement, 1)
shortcut.write_text(s)
