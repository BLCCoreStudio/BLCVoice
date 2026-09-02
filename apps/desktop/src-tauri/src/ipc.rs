use std::sync::Arc;

use blcvoice_audio::{
    AudioDeviceId, AudioFailure, AudioFailureKind, AudioSampleFormat, AudioStreamConfig,
    CaptureStats, InputCaptureFactory, InputDeviceDiscovery, InputDeviceInfo, InputDiscovery,
};
use blcvoice_audio_cpal::{CpalInputCaptureFactory, CpalInputDeviceDiscovery};
use blcvoice_core::{SessionId, SessionSnapshot};
use serde::Serialize;
use tauri::State;

use crate::capture::{
    DesktopCaptureError, DesktopCaptureErrorKind, DesktopCaptureService, MicrophoneTestReport,
    session_state_name,
};

pub struct DesktopState {
    capture: Arc<DesktopCaptureService>,
}

impl DesktopState {
    #[must_use]
    pub fn production() -> Self {
        let discovery: Arc<dyn InputDeviceDiscovery> = Arc::new(CpalInputDeviceDiscovery);
        let capture_factory: Arc<dyn InputCaptureFactory> = Arc::new(CpalInputCaptureFactory);
        Self {
            capture: Arc::new(DesktopCaptureService::new(discovery, capture_factory)),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandErrorDto {
    code: &'static str,
    message: String,
}

impl CommandErrorDto {
    fn blocking_worker(message: impl Into<String>) -> Self {
        Self {
            code: "blocking_worker_failed",
            message: message.into(),
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
        Self {
            code,
            message: error.message().to_owned(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopStatusDto {
    session: Option<SessionDto>,
    last_pump_failure: Option<String>,
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
    run_blocking(move || Ok(InputDiscoveryDto::from(capture.discover_input_devices()))).await
}

#[tauri::command]
pub async fn microphone_test_start(
    state: State<'_, DesktopState>,
    device_id: String,
) -> Result<SessionDto, CommandErrorDto> {
    let capture = Arc::clone(&state.capture);
    run_blocking(move || {
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
    run_blocking(move || {
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
    run_blocking(move || {
        capture
            .cancel_microphone_test(SessionId::new(session_id))
            .map(SessionDto::from)
    })
    .await
}

#[tauri::command]
pub fn desktop_status(state: State<'_, DesktopState>) -> DesktopStatusDto {
    DesktopStatusDto {
        session: state.capture.current_session().map(SessionDto::from),
        last_pump_failure: state.capture.last_pump_failure(),
    }
}

async fn run_blocking<T, F>(task: F) -> Result<T, CommandErrorDto>
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
