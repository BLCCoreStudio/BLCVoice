use std::path::PathBuf;
use std::sync::Arc;

use blcvoice_asr::RecognitionOptions;
use blcvoice_audio::{
    AudioDeviceId, AudioFailure, AudioFailureKind, AudioSampleFormat, AudioStreamConfig,
    CaptureStats, InputCaptureFactory, InputDeviceDiscovery, InputDeviceInfo, InputDiscovery,
};
use blcvoice_audio_cpal::{CpalInputCaptureFactory, CpalInputDeviceDiscovery};
use blcvoice_core::{SessionId, SessionSnapshot};
use blcvoice_insertion::{InsertionError, InsertionErrorKind, InsertionReceipt};
use serde::Serialize;
use tauri::State;

use crate::capture::{
    DesktopCaptureError, DesktopCaptureErrorKind, DesktopCaptureService, MicrophoneTestReport,
    session_state_name,
};
use crate::dictation::{
    DesktopDictationError, DesktopDictationErrorKind, DesktopDictationReport,
    DesktopDictationRequest, DesktopDictationService,
};
use crate::insertion::DesktopInsertionService;
use crate::models::{ModelError, ModelErrorKind, ModelManager, ModelStatus};
use crate::settings::{AppSettings, SettingsError, SettingsService};

pub struct DesktopState {
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
        let settings =
            Arc::new(SettingsService::open(config_dir).map_err(|error| error.to_string())?);
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandErrorDto {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    recoverable_text: Option<String>,
}

impl CommandErrorDto {
    fn plain(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recoverable_text: None,
        }
    }

    fn blocking_worker(message: impl Into<String>) -> Self {
        Self::plain("blocking_worker_failed", message)
    }

    fn insertion(error: InsertionError, recoverable_text: String) -> Self {
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
}

impl From<DesktopCaptureError> for CommandErrorDto {
    fn from(error: DesktopCaptureError) -> Self {
        let code = match error.kind() {
            DesktopCaptureErrorKind::Busy => "capture_busy",
            DesktopCaptureErrorKind::InvalidDevice => "invalid_device",
            DesktopCaptureErrorKind::StaleSession => "stale_session",
            DesktopCaptureErrorKind::PumpFailed => "capture_pump_failed",
            DesktopCaptureErrorKind::WorkerSpawn => "capture_worker_spawn_failed",
            DesktopCaptureErrorKind::WorkerJoin => "capture_worker_join_failed",
            DesktopCaptureErrorKind::Runtime => "dictation_runtime_failed",
        };
        Self::plain(code, error.message())
    }
}

impl From<DesktopDictationError> for CommandErrorDto {
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatusDto {
    session: Option<SessionDto>,
    last_pump_failure: Option<String>,
    dictation_state: &'static str,
    dictation_session_id: Option<u64>,
    insertion: InsertionCapabilityDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertionCapabilityDto {
    backend: Option<String>,
    authorization: Option<String>,
    semantic_delivery_verifiable: bool,
    available: bool,
    error: Option<String>,
}

impl InsertionCapabilityDto {
    fn from_service(service: &DesktopInsertionService) -> Self {
        match service.capability() {
            Ok(capability) => Self {
                backend: Some(capability.backend().to_string()),
                authorization: Some(capability.authorization().to_string()),
                semantic_delivery_verifiable: capability.semantic_delivery_verifiable(),
                available: true,
                error: None,
            },
            Err(error) => Self {
                backend: None,
                authorization: None,
                semantic_delivery_verifiable: false,
                available: false,
                error: Some(error.to_string()),
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDto {
    id: u64,
    state: &'static str,
    failure_stage: Option<String>,
}

impl From<SessionSnapshot> for SessionDto {
    fn from(session: SessionSnapshot) -> Self {
        Self {
            id: session.id.get(),
            state: session_state_name(session.state),
            failure_stage: session.failure_stage.map(|stage| format!("{stage:?}")),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDiscoveryDto {
    selected_backend: Option<String>,
    devices: Vec<InputDeviceDto>,
    failures: Vec<AudioFailureDto>,
}

impl From<InputDiscovery> for InputDiscoveryDto {
    fn from(discovery: InputDiscovery) -> Self {
        Self {
            selected_backend: discovery
                .selected_backend
                .map(|backend| backend.to_string()),
            devices: discovery
                .devices
                .into_iter()
                .map(InputDeviceDto::from)
                .collect(),
            failures: discovery
                .failures
                .into_iter()
                .map(AudioFailureDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceDto {
    id: String,
    name: String,
    backend: String,
    is_default: bool,
    default_config: Option<AudioStreamConfigDto>,
}

impl From<InputDeviceInfo> for InputDeviceDto {
    fn from(device: InputDeviceInfo) -> Self {
        Self {
            id: device.id.to_string(),
            name: device.name,
            backend: device.backend.to_string(),
            is_default: device.is_default,
            default_config: device.default_config.map(AudioStreamConfigDto::from),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStreamConfigDto {
    channels: u16,
    sample_rate_hz: u32,
    sample_format: String,
}

impl From<AudioStreamConfig> for AudioStreamConfigDto {
    fn from(config: AudioStreamConfig) -> Self {
        Self {
            channels: config.channels,
            sample_rate_hz: config.sample_rate_hz,
            sample_format: sample_format_name(config.sample_format),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFailureDto {
    backend: Option<String>,
    device_id: Option<String>,
    kind: &'static str,
    message: String,
}

impl From<AudioFailure> for AudioFailureDto {
    fn from(failure: AudioFailure) -> Self {
        Self {
            backend: failure.backend.map(|backend| backend.to_string()),
            device_id: failure.device_id.map(|device_id| device_id.to_string()),
            kind: audio_failure_kind_name(failure.kind),
            message: failure.message,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneTestReportDto {
    session_id: u64,
    captured_frames: usize,
    capture_stats: CaptureStatsDto,
    finalized_state: &'static str,
    terminal_state: &'static str,
}

impl From<MicrophoneTestReport> for MicrophoneTestReportDto {
    fn from(report: MicrophoneTestReport) -> Self {
        Self {
            session_id: report.finalized.session.id.get(),
            captured_frames: report.finalized.source_frames,
            capture_stats: CaptureStatsDto::from(report.finalized.capture_stats),
            finalized_state: session_state_name(report.finalized.session.state),
            terminal_state: session_state_name(report.terminal_session.state),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationReportDto {
    session_id: u64,
    state: &'static str,
    text: String,
    raw_text: Option<String>,
    detected_language: Option<String>,
    engine_id: String,
    model_id: String,
    backend_name: String,
    source_frames: usize,
    asr_frames: usize,
    capture_stats: CaptureStatsDto,
    insertion_backend: String,
    submitted_utf8_bytes: usize,
    semantic_delivery_verified: bool,
}

impl DictationReportDto {
    fn completed(
        report: DesktopDictationReport,
        receipt: InsertionReceipt,
        completed: SessionSnapshot,
    ) -> Self {
        let capture = report.transcription.capture;
        let transcription = capture.transcription;
        Self {
            session_id: completed.id.get(),
            state: session_state_name(completed.state),
            text: transcription.text,
            raw_text: transcription.raw_text,
            detected_language: transcription.detected_language,
            engine_id: report.engine_id,
            model_id: report.model_id,
            backend_name: report.backend_name,
            source_frames: capture.source_frames,
            asr_frames: capture.asr_frames,
            capture_stats: CaptureStatsDto::from(capture.capture_stats),
            insertion_backend: receipt.backend().to_string(),
            submitted_utf8_bytes: receipt.submitted_utf8_bytes(),
            semantic_delivery_verified: receipt.semantic_delivery_verified(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatsDto {
    received_samples: u64,
    dropped_samples: u64,
    callback_errors: u64,
    last_failure: Option<String>,
}

impl From<CaptureStats> for CaptureStatsDto {
    fn from(stats: CaptureStats) -> Self {
        Self {
            received_samples: stats.received_samples,
            dropped_samples: stats.dropped_samples,
            callback_errors: stats.callback_errors,
            last_failure: stats
                .last_failure
                .map(|failure| audio_failure_kind_name(failure).to_owned()),
        }
    }
}

#[tauri::command]
pub async fn audio_input_discovery(
    state: State<'_, DesktopState>,
) -> Result<InputDiscoveryDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    run_capture_blocking(move || Ok(InputDiscoveryDto::from(capture.discover_input_devices())))
        .await
}

#[tauri::command]
pub async fn microphone_test_start(
    state: State<'_, DesktopState>,
    device_id: String,
) -> Result<SessionDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    run_capture_blocking(move || {
        let device_id = AudioDeviceId::new(device_id).map_err(|error| {
            DesktopCaptureError::new(DesktopCaptureErrorKind::InvalidDevice, error.to_string())
        })?;
        capture
            .start_microphone_test(device_id)
            .map(SessionDto::from)
    })
    .await
}

#[tauri::command]
pub async fn microphone_test_finish(
    state: State<'_, DesktopState>,
    session_id: u64,
) -> Result<MicrophoneTestReportDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    run_capture_blocking(move || {
        capture
            .finish_microphone_test(SessionId::new(session_id))
            .map(MicrophoneTestReportDto::from)
    })
    .await
}

#[tauri::command]
pub async fn microphone_test_cancel(
    state: State<'_, DesktopState>,
    session_id: u64,
) -> Result<SessionDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    run_capture_blocking(move || {
        capture
            .cancel_microphone_test(SessionId::new(session_id))
            .map(SessionDto::from)
    })
    .await
}

#[tauri::command]
pub async fn dictation_start(
    state: State<'_, DesktopState>,
    device_id: String,
    model_path: String,
    language_hint: Option<String>,
) -> Result<SessionDto, CommandErrorDto> {
    let dictation = Arc::clone(&state.dictation);
    run_dictation_blocking(move || {
        let device_id = AudioDeviceId::new(device_id).map_err(|error| {
            DesktopDictationError::new(
                DesktopDictationErrorKind::InvalidConfiguration,
                error.to_string(),
            )
        })?;
        let recognition = RecognitionOptions {
            language_hint,
            ..RecognitionOptions::default()
        };
        dictation
            .start(DesktopDictationRequest {
                device_id,
                model_path: PathBuf::from(model_path),
                recognition,
            })
            .map(SessionDto::from)
    })
    .await
}

#[tauri::command]
pub async fn dictation_finish(
    state: State<'_, DesktopState>,
    session_id: u64,
) -> Result<DictationReportDto, CommandErrorDto> {
    let dictation = Arc::clone(&state.dictation);
    let insertion = Arc::clone(&state.insertion);
    tauri::async_runtime::spawn_blocking(move || {
        let session_id = SessionId::new(session_id);
        let report = dictation.finish(session_id).map_err(CommandErrorDto::from)?;
        let text = report.transcription.capture.transcription.text.clone();
        dictation
            .begin_insertion(session_id)
            .map_err(CommandErrorDto::from)?;

        let receipt = match insertion.insert_text(&text) {
            Ok(receipt) => receipt,
            Err(error) => {
                let lifecycle_failure = dictation.fail_insertion(session_id).err();
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

        let completed = dictation
            .complete_insertion(session_id)
            .map_err(CommandErrorDto::from)?;
        Ok(DictationReportDto::completed(report, receipt, completed))
    })
    .await
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
    })?
}

#[tauri::command]
pub async fn dictation_cancel(
    state: State<'_, DesktopState>,
    session_id: u64,
) -> Result<SessionDto, CommandErrorDto> {
    let dictation = Arc::clone(&state.dictation);
    run_dictation_blocking(move || {
        dictation
            .cancel(SessionId::new(session_id))
            .map(SessionDto::from)
    })
    .await
}

#[derive(Debug, Serialize)]
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
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("dictation worker failed: {error}"))
    })?
}

#[tauri::command]
pub fn insertion_capability(state: State<'_, DesktopState>) -> InsertionCapabilityDto {
    InsertionCapabilityDto::from_service(&state.insertion)
}

#[tauri::command]
pub fn desktop_status(state: State<'_, DesktopState>) -> DesktopStatusDto {
    DesktopStatusDto {
        session: state.capture.current_session().map(SessionDto::from),
        last_pump_failure: state.capture.last_pump_failure(),
        dictation_state: state.dictation.state_name(),
        dictation_session_id: state.dictation.active_session_id().map(SessionId::get),
        insertion: InsertionCapabilityDto::from_service(&state.insertion),
    }
}

async fn run_capture_blocking<T, F>(task: F) -> Result<T, CommandErrorDto>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, DesktopCaptureError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| {
            CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
        })?
        .map_err(CommandErrorDto::from)
}

async fn run_dictation_blocking<T, F>(task: F) -> Result<T, CommandErrorDto>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, DesktopDictationError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|error| {
            CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
        })?
        .map_err(CommandErrorDto::from)
}

fn sample_format_name(format: AudioSampleFormat) -> String {
    match format {
        AudioSampleFormat::F32 => "f32".to_owned(),
        AudioSampleFormat::F64 => "f64".to_owned(),
        AudioSampleFormat::I8 => "i8".to_owned(),
        AudioSampleFormat::I16 => "i16".to_owned(),
        AudioSampleFormat::I24 => "i24".to_owned(),
        AudioSampleFormat::I32 => "i32".to_owned(),
        AudioSampleFormat::I64 => "i64".to_owned(),
        AudioSampleFormat::U8 => "u8".to_owned(),
        AudioSampleFormat::U16 => "u16".to_owned(),
        AudioSampleFormat::U24 => "u24".to_owned(),
        AudioSampleFormat::U32 => "u32".to_owned(),
        AudioSampleFormat::U64 => "u64".to_owned(),
        AudioSampleFormat::DsdU8 => "dsd-u8".to_owned(),
        AudioSampleFormat::DsdU16 => "dsd-u16".to_owned(),
        AudioSampleFormat::DsdU32 => "dsd-u32".to_owned(),
        AudioSampleFormat::Other(name) => name,
    }
}

const fn audio_failure_kind_name(kind: AudioFailureKind) -> &'static str {
    match kind {
        AudioFailureKind::NoInputDevices => "no_input_devices",
        AudioFailureKind::DeviceBusy => "device_busy",
        AudioFailureKind::DeviceChanged => "device_changed",
        AudioFailureKind::DeviceNotAvailable => "device_not_available",
        AudioFailureKind::BackendUnavailable => "backend_unavailable",
        AudioFailureKind::InvalidInput => "invalid_input",
        AudioFailureKind::PermissionDenied => "permission_denied",
        AudioFailureKind::RealtimeDenied => "realtime_denied",
        AudioFailureKind::ResourceExhausted => "resource_exhausted",
        AudioFailureKind::StreamInvalidated => "stream_invalidated",
        AudioFailureKind::UnsupportedConfig => "unsupported_config",
        AudioFailureKind::UnsupportedOperation => "unsupported_operation",
        AudioFailureKind::Xrun => "xrun",
        AudioFailureKind::BackendError => "backend_error",
        AudioFailureKind::Other => "other",
    }
}
