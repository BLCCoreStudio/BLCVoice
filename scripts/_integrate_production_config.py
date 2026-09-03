from pathlib import Path

lib = Path("apps/desktop/src-tauri/src/lib.rs")
s = lib.read_text()
s = s.replace("mod insertion;\nmod ipc;\nmod shortcut;\n", "mod insertion;\nmod ipc;\nmod models;\nmod settings;\nmod shortcut;\n")
s = s.replace(
    """use ipc::{
    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,
    dictation_start, insertion_capability, microphone_test_cancel, microphone_test_finish,
    microphone_test_start,
};""",
    """use ipc::{
    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,
    dictation_start, dictation_start_configured, insertion_capability, microphone_test_cancel,
    microphone_test_finish, microphone_test_start, model_catalog, model_install, model_remove,
    settings_get, settings_set_input_device, settings_set_language_hint, settings_set_model,
};""",
)
s = s.replace(
    "use shortcut::{ShortcutService, install_shortcut_backend, shortcut_capability};",
    "use shortcut::{ShortcutService, install_shortcut_backend, shortcut_capability};\nuse tauri::Manager;",
)
s = s.replace(
    """        .manage(DesktopState::production())
        .manage(ShortcutService::production())
        .setup(|app| {
            install_shortcut_backend(app);
            Ok(())
        })""",
    """        .manage(ShortcutService::production())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let desktop = DesktopState::production(config_dir, data_dir)
                .map_err(std::io::Error::other)?;
            app.manage(desktop);
            install_shortcut_backend(app);
            Ok(())
        })""",
)
s = s.replace(
    "            dictation_start,\n            dictation_finish,",
    "            dictation_start,\n            dictation_start_configured,\n            dictation_finish,",
)
s = s.replace(
    "            insertion_capability,\n            shortcut_capability,",
    """            insertion_capability,
            settings_get,
            settings_set_input_device,
            settings_set_model,
            settings_set_language_hint,
            model_catalog,
            model_install,
            model_remove,
            shortcut_capability,""",
)
lib.write_text(s)

ipc = Path("apps/desktop/src-tauri/src/ipc.rs")
s = ipc.read_text()
s = s.replace(
    "use crate::insertion::DesktopInsertionService;\n",
    """use crate::insertion::DesktopInsertionService;
use crate::models::{ModelError, ModelErrorKind, ModelManager, ModelStatus};
use crate::settings::{AppSettings, SettingsError, SettingsService};
""",
)
old = """pub struct DesktopState {
    capture: Arc<DesktopCaptureService>,
    dictation: Arc<DesktopDictationService>,
    insertion: Arc<DesktopInsertionService>,
}

impl DesktopState {
    #[must_use]
    pub fn production() -> Self {
        let discovery: Arc<dyn InputDeviceDiscovery> = Arc::new(CpalInputDeviceDiscovery);
        let capture_factory: Arc<dyn InputCaptureFactory> = Arc::new(CpalInputCaptureFactory);
        let capture = Arc::new(DesktopCaptureService::new(discovery, capture_factory));
        let dictation = Arc::new(DesktopDictationService::production(Arc::clone(&capture)));
        let insertion = Arc::new(DesktopInsertionService::production());
        Self {
            capture,
            dictation,
            insertion,
        }
    }
}
"""
new = """pub struct DesktopState {
    capture: Arc<DesktopCaptureService>,
    dictation: Arc<DesktopDictationService>,
    insertion: Arc<DesktopInsertionService>,
    settings: Arc<SettingsService>,
    models: Arc<ModelManager>,
}

impl DesktopState {
    pub fn production(config_dir: PathBuf, data_dir: PathBuf) -> Result<Self, String> {
        let discovery: Arc<dyn InputDeviceDiscovery> = Arc::new(CpalInputDeviceDiscovery);
        let capture_factory: Arc<dyn InputCaptureFactory> = Arc::new(CpalInputCaptureFactory);
        let capture = Arc::new(DesktopCaptureService::new(discovery, capture_factory));
        let dictation = Arc::new(DesktopDictationService::production(Arc::clone(&capture)));
        let insertion = Arc::new(DesktopInsertionService::production());
        let settings = Arc::new(SettingsService::open(config_dir).map_err(|error| error.to_string())?);
        let models = Arc::new(ModelManager::new(data_dir).map_err(|error| error.to_string())?);
        Ok(Self {
            capture,
            dictation,
            insertion,
            settings,
            models,
        })
    }
}
"""
if old not in s:
    raise SystemExit("DesktopState block not found")
s = s.replace(old, new)

marker = """impl From<DesktopDictationError> for CommandErrorDto {
    fn from(error: DesktopDictationError) -> Self {
        let code = match error.kind() {
            DesktopDictationErrorKind::Busy => "dictation_busy",
            DesktopDictationErrorKind::InvalidConfiguration => "dictation_invalid_configuration",
            DesktopDictationErrorKind::StaleSession => "stale_session",
            DesktopDictationErrorKind::RecognizerLoad => "recognizer_load_failed",
            DesktopDictationErrorKind::Capture => "dictation_capture_failed",
            DesktopDictationErrorKind::Transcription => "dictation_transcription_failed",
            DesktopDictationErrorKind::Insertion => "dictation_insertion_lifecycle_failed",
        };
        Self::plain(code, error.message())
    }
}
"""
addition = marker + """
impl From<SettingsError> for CommandErrorDto {
    fn from(error: SettingsError) -> Self {
        Self::plain("settings_failed", error.message())
    }
}

impl From<ModelError> for CommandErrorDto {
    fn from(error: ModelError) -> Self {
        let code = match error.kind() {
            ModelErrorKind::UnknownModel => "unknown_model",
            ModelErrorKind::Busy => "model_busy",
            ModelErrorKind::Network => "model_network_failed",
            ModelErrorKind::DownloadInvalid => "model_download_invalid",
            ModelErrorKind::Validation => "model_validation_failed",
            ModelErrorKind::Io => "model_io_failed",
        };
        Self::plain(code, error.message())
    }
}
"""
if marker not in s:
    raise SystemExit("dictation error impl not found")
s = s.replace(marker, addition)

insert_marker = "#[tauri::command]\npub fn insertion_capability"
commands = r'''#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusDto {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    tier: &'static str,
    advertised_bytes: u64,
    installed: bool,
    installed_bytes: Option<u64>,
    recommended: bool,
    selected: bool,
}

impl ModelStatusDto {
    fn from_status(status: ModelStatus, selected_model_id: Option<&str>) -> Self {
        Self {
            id: status.spec.id(),
            name: status.spec.name(),
            description: status.spec.description(),
            tier: status.spec.tier().as_str(),
            advertised_bytes: status.spec.advertised_bytes(),
            installed: status.installed,
            installed_bytes: status.installed_bytes,
            recommended: status.recommended,
            selected: selected_model_id == Some(status.spec.id()),
        }
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, DesktopState>) -> AppSettings {
    state.settings.snapshot()
}

#[tauri::command]
pub fn settings_set_input_device(
    state: State<'_, DesktopState>,
    device_id: Option<String>,
) -> Result<AppSettings, CommandErrorDto> {
    if let Some(device_id) = device_id.as_deref() {
        let discovery = state.capture.discover_input_devices();
        if !discovery
            .devices
            .iter()
            .any(|device| device.id.as_str() == device_id)
        {
            return Err(CommandErrorDto::plain(
                "invalid_device",
                "selected microphone is not currently available",
            ));
        }
    }
    state
        .settings
        .set_input_device(device_id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub fn settings_set_model(
    state: State<'_, DesktopState>,
    model_id: Option<String>,
) -> Result<AppSettings, CommandErrorDto> {
    if let Some(model_id) = model_id.as_deref()
        && state
            .models
            .installed_model_path(model_id)
            .map_err(CommandErrorDto::from)?
            .is_none()
    {
        return Err(CommandErrorDto::plain(
            "model_not_installed",
            "selected speech model is not installed",
        ));
    }
    state
        .settings
        .set_model(model_id)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub fn settings_set_language_hint(
    state: State<'_, DesktopState>,
    language_hint: Option<String>,
) -> Result<AppSettings, CommandErrorDto> {
    state
        .settings
        .set_language_hint(language_hint)
        .map_err(CommandErrorDto::from)
}

#[tauri::command]
pub fn model_catalog(state: State<'_, DesktopState>) -> Vec<ModelStatusDto> {
    let settings = state.settings.snapshot();
    state
        .models
        .catalog()
        .into_iter()
        .map(|status| ModelStatusDto::from_status(status, settings.selected_model_id()))
        .collect()
}

#[tauri::command]
pub async fn model_install(
    state: State<'_, DesktopState>,
    model_id: String,
) -> Result<ModelStatusDto, CommandErrorDto> {
    let models = Arc::clone(&state.models);
    let settings = Arc::clone(&state.settings);
    tauri::async_runtime::spawn_blocking(move || {
        let status = models.install(&model_id).map_err(CommandErrorDto::from)?;
        let mut snapshot = settings.snapshot();
        if snapshot.selected_model_id().is_none() {
            snapshot = settings
                .set_model(Some(status.spec.id().to_owned()))
                .map_err(CommandErrorDto::from)?;
        }
        Ok(ModelStatusDto::from_status(
            status,
            snapshot.selected_model_id(),
        ))
    })
    .await
    .map_err(|error| CommandErrorDto::blocking_worker(format!("model worker failed: {error}")))?
}

#[tauri::command]
pub async fn model_remove(
    state: State<'_, DesktopState>,
    model_id: String,
) -> Result<ModelStatusDto, CommandErrorDto> {
    let models = Arc::clone(&state.models);
    let settings = Arc::clone(&state.settings);
    tauri::async_runtime::spawn_blocking(move || {
        let status = models.remove(&model_id).map_err(CommandErrorDto::from)?;
        let mut snapshot = settings.snapshot();
        if snapshot.selected_model_id() == Some(model_id.as_str()) {
            snapshot = settings.set_model(None).map_err(CommandErrorDto::from)?;
        }
        Ok(ModelStatusDto::from_status(
            status,
            snapshot.selected_model_id(),
        ))
    })
    .await
    .map_err(|error| CommandErrorDto::blocking_worker(format!("model worker failed: {error}")))?
}

#[tauri::command]
pub async fn dictation_start_configured(
    state: State<'_, DesktopState>,
) -> Result<SessionDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    let dictation = Arc::clone(&state.dictation);
    let settings = Arc::clone(&state.settings);
    let models = Arc::clone(&state.models);
    tauri::async_runtime::spawn_blocking(move || {
        let mut snapshot = settings.snapshot();
        let discovery = capture.discover_input_devices();
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
            snapshot = settings
                .set_input_device(Some(device_id.to_string()))
                .map_err(CommandErrorDto::from)?;
        }

        let selected_model = snapshot.selected_model_id().and_then(|id| {
            models
                .installed_model_path(id)
                .ok()
                .flatten()
                .map(|path| (id.to_owned(), path))
        });
        let (model_id, model_path) = if let Some(selected) = selected_model {
            selected
        } else {
            let catalog = models.catalog();
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
            let path = models
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
            snapshot = settings
                .set_model(Some(model_id))
                .map_err(CommandErrorDto::from)?;
        }

        let recognition = RecognitionOptions {
            language_hint: snapshot.language_hint().map(str::to_owned),
            ..RecognitionOptions::default()
        };
        dictation
            .start(DesktopDictationRequest {
                device_id,
                model_path,
                recognition,
            })
            .map(SessionDto::from)
            .map_err(CommandErrorDto::from)
    })
    .await
    .map_err(|error| CommandErrorDto::blocking_worker(format!("dictation worker failed: {error}")))?
}

'''
if insert_marker not in s:
    raise SystemExit("insertion command marker not found")
s = s.replace(insert_marker, commands + insert_marker)
ipc.write_text(s)
