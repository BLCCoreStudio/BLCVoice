use std::error::Error;
use std::fmt;
use std::fs;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use blcvoice_asr::{RecognitionError, RecognitionOptions, SpeechRecognizer};
use blcvoice_asr_transcribe::{TranscribeRecognizer, TranscribeRecognizerConfig};
use blcvoice_audio::AudioDeviceId;
use blcvoice_core::{SessionId, SessionSnapshot};
use blcvoice_dictation::SpeechDetectionReport;
use blcvoice_runtime::{FinalizationReport, RuntimeTranscription, RuntimeVadTranscriptionOutcome};
use blcvoice_vad::{VadConfig, VoiceActivityDetector};
use blcvoice_vad_silero::SileroVoiceActivityDetector;

use crate::capture::{DesktopCaptureError, DesktopCaptureErrorKind, DesktopCaptureService};

pub const DEFAULT_DICTATION_MAX_DURATION_MS: u32 = 300_000;

#[derive(Debug, Clone)]
pub struct DesktopDictationRequest {
    pub device_id: AudioDeviceId,
    pub model_path: PathBuf,
    pub recognition: RecognitionOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopDictationErrorKind {
    Busy,
    InvalidConfiguration,
    StaleSession,
    RecognizerLoad,
    Capture,
    SpeechDetection,
    Transcription,
    Insertion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopDictationError {
    kind: DesktopDictationErrorKind,
    message: String,
}

impl DesktopDictationError {
    #[must_use]
    pub fn new(kind: DesktopDictationErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> DesktopDictationErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DesktopDictationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for DesktopDictationError {}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopDictationReport {
    pub finalized: FinalizationReport,
    pub transcription: RuntimeTranscription,
    pub detection: SpeechDetectionReport,
    pub vad_backend: String,
    pub engine_id: String,
    pub model_id: String,
    pub backend_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopNoSpeechReport {
    pub finalized: FinalizationReport,
    pub terminal_session: SessionSnapshot,
    pub detection: SpeechDetectionReport,
    pub vad_backend: String,
    pub engine_id: String,
    pub model_id: String,
    pub backend_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DesktopDictationFinish {
    NoSpeech(DesktopNoSpeechReport),
    Transcribed(DesktopDictationReport),
}

type VadFactory = dyn Fn() -> Box<dyn VoiceActivityDetector> + Send + Sync;

trait RecognizerFactory: Send + Sync {
    fn load(&self, model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError>;
}

#[derive(Debug, Default)]
struct TranscribeRecognizerFactory;

impl RecognizerFactory for TranscribeRecognizerFactory {
    fn load(&self, model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
        TranscribeRecognizer::load(model_path, TranscribeRecognizerConfig::default())
            .map(|recognizer| Box::new(recognizer) as Box<dyn SpeechRecognizer>)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecognizerCacheKey {
    model_path: PathBuf,
    file_len: Option<u64>,
    modified: Option<SystemTime>,
}

impl RecognizerCacheKey {
    fn for_path(model_path: &Path) -> Self {
        let resolved_path =
            fs::canonicalize(model_path).unwrap_or_else(|_| model_path.to_path_buf());
        let metadata = fs::metadata(&resolved_path).ok();
        Self {
            model_path: resolved_path,
            file_len: metadata.as_ref().map(fs::Metadata::len),
            modified: metadata.and_then(|metadata| metadata.modified().ok()),
        }
    }
}

struct CachedRecognizer {
    key: RecognizerCacheKey,
    recognizer: Box<dyn SpeechRecognizer>,
}

struct ActiveDictation {
    session_id: SessionId,
    recognizer_key: RecognizerCacheKey,
    recognizer: Box<dyn SpeechRecognizer>,
    recognition: RecognitionOptions,
}

impl fmt::Debug for ActiveDictation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActiveDictation")
            .field("session_id", &self.session_id)
            .field("engine_id", &self.recognizer.engine_id())
            .field("model_id", &self.recognizer.model_id())
            .field("backend_name", &self.recognizer.backend_name())
            .field("recognition", &self.recognition)
            .finish()
    }
}

#[derive(Debug, Default)]
enum DictationSlot {
    #[default]
    Idle,
    Preparing,
    Recording(ActiveDictation),
    Finalizing(SessionId),
    AwaitingInsertion(SessionId),
    Inserting(SessionId),
}

impl DictationSlot {
    fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::Recording(active) => Some(active.session_id),
            Self::Finalizing(session_id)
            | Self::AwaitingInsertion(session_id)
            | Self::Inserting(session_id) => Some(*session_id),
            Self::Idle | Self::Preparing => None,
        }
    }

    const fn state_name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Preparing => "preparing",
            Self::Recording(_) => "recording",
            Self::Finalizing(_) => "finalizing",
            Self::AwaitingInsertion(_) => "awaitingInsertion",
            Self::Inserting(_) => "inserting",
        }
    }
}

pub struct DesktopDictationService {
    capture: Arc<DesktopCaptureService>,
    recognizers: Arc<dyn RecognizerFactory>,
    vad_factory: Arc<VadFactory>,
    recognizer_cache: Mutex<Option<CachedRecognizer>>,
    slot: Mutex<DictationSlot>,
}

impl fmt::Debug for DesktopDictationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let slot = self.lock_slot();
        formatter
            .debug_struct("DesktopDictationService")
            .field("state", &slot.state_name())
            .field("session_id", &slot.session_id())
            .finish_non_exhaustive()
    }
}

impl DesktopDictationService {
    #[must_use]
    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {
        Self::new(
            capture,
            Arc::new(TranscribeRecognizerFactory),
            Arc::new(|| Box::new(SileroVoiceActivityDetector::new())),
        )
    }

    fn new(
        capture: Arc<DesktopCaptureService>,
        recognizers: Arc<dyn RecognizerFactory>,
        vad_factory: Arc<VadFactory>,
    ) -> Self {
        Self {
            capture,
            recognizers,
            vad_factory,
            recognizer_cache: Mutex::new(None),
            slot: Mutex::new(DictationSlot::Idle),
        }
    }

    pub fn start(
        &self,
        request: DesktopDictationRequest,
    ) -> Result<SessionSnapshot, DesktopDictationError> {
        if request.model_path.as_os_str().is_empty() {
            return Err(DesktopDictationError::new(
                DesktopDictationErrorKind::InvalidConfiguration,
                "dictation model path cannot be empty",
            ));
        }
        if request
            .recognition
            .language_hint
            .as_deref()
            .is_some_and(|language| language.trim().is_empty())
        {
            return Err(DesktopDictationError::new(
                DesktopDictationErrorKind::InvalidConfiguration,
                "dictation language hint cannot be empty",
            ));
        }

        {
            let mut slot = self.lock_slot();
            if !matches!(*slot, DictationSlot::Idle) {
                return Err(busy_error(slot.state_name()));
            }
            *slot = DictationSlot::Preparing;
        }

        let (recognizer_key, recognizer) = match self.acquire_recognizer(&request.model_path) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::RecognizerLoad,
                    format!("could not load dictation model: {error}"),
                ));
            }
        };

        let session = match self
            .capture
            .start_dictation_recording(request.device_id, DEFAULT_DICTATION_MAX_DURATION_MS)
        {
            Ok(session) => session,
            Err(error) => {
                self.recycle_recognizer(recognizer_key, recognizer);
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let mut slot = self.lock_slot();
        if !matches!(*slot, DictationSlot::Preparing) {
            drop(slot);
            self.recycle_recognizer(recognizer_key, recognizer);
            let _ = self.capture.cancel_dictation(session.id);
            self.reset_to_idle();
            return Err(DesktopDictationError::new(
                DesktopDictationErrorKind::Busy,
                "dictation preparation was invalidated before recording started",
            ));
        }
        *slot = DictationSlot::Recording(ActiveDictation {
            session_id: session.id,
            recognizer_key,
            recognizer,
            recognition: request.recognition,
        });

        Ok(session)
    }

    pub fn finish(
        &self,
        session_id: SessionId,
    ) -> Result<DesktopDictationFinish, DesktopDictationError> {
        let mut active = {
            let mut slot = self.lock_slot();
            match mem::replace(&mut *slot, DictationSlot::Finalizing(session_id)) {
                DictationSlot::Recording(active) if active.session_id == session_id => active,
                previous => {
                    let error = slot_error(&previous, session_id);
                    *slot = previous;
                    return Err(error);
                }
            }
        };

        let finalized = match self.capture.finish_dictation_recording(session_id) {
            Ok(finalized) => finalized,
            Err(error) => {
                self.recycle_recognizer(active.recognizer_key, active.recognizer);
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let engine_id = active.recognizer.engine_id().to_owned();
        let model_id = active.recognizer.model_id().to_owned();
        let backend_name = active.recognizer.backend_name().to_owned();
        let mut detector = (self.vad_factory)();
        let vad_backend = detector.backend_name().to_owned();
        let transcription_result = self.capture.transcribe_dictation_with_vad(
            session_id,
            detector.as_mut(),
            VadConfig::default(),
            active.recognizer.as_mut(),
            &active.recognition,
        );
        self.recycle_recognizer(active.recognizer_key, active.recognizer);
        let outcome = match transcription_result {
            Ok(outcome) => outcome,
            Err(error) => {
                let detection_failed = error.kind() == DesktopCaptureErrorKind::SpeechDetection;
                if detection_failed {
                    let _ = self.capture.fail_dictation_speech_detection(session_id);
                } else {
                    let _ = self.capture.fail_dictation_recognition(session_id);
                }
                self.reset_to_idle();
                let kind = if detection_failed {
                    DesktopDictationErrorKind::SpeechDetection
                } else {
                    DesktopDictationErrorKind::Transcription
                };
                return Err(DesktopDictationError::new(
                    kind,
                    format!("dictation processing failed: {error}"),
                ));
            }
        };

        match outcome {
            RuntimeVadTranscriptionOutcome::NoSpeech { session, detection } => {
                self.reset_to_idle();
                Ok(DesktopDictationFinish::NoSpeech(DesktopNoSpeechReport {
                    finalized,
                    terminal_session: session,
                    detection,
                    vad_backend,
                    engine_id,
                    model_id,
                    backend_name,
                }))
            }
            RuntimeVadTranscriptionOutcome::Transcribed {
                transcription,
                detection,
            } => {
                *self.lock_slot() = DictationSlot::AwaitingInsertion(session_id);
                Ok(DesktopDictationFinish::Transcribed(
                    DesktopDictationReport {
                        finalized,
                        transcription,
                        detection,
                        vad_backend,
                        engine_id,
                        model_id,
                        backend_name,
                    },
                ))
            }
        }
    }

    pub fn begin_insertion(&self, session_id: SessionId) -> Result<(), DesktopDictationError> {
        let mut slot = self.lock_slot();
        match mem::replace(&mut *slot, DictationSlot::Inserting(session_id)) {
            DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => Ok(()),
            previous => {
                let error = slot_error(&previous, session_id);
                *slot = previous;
                Err(error)
            }
        }
    }

    pub fn complete_insertion(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, DesktopDictationError> {
        self.ensure_inserting(session_id)?;
        let result = self
            .capture
            .mark_dictation_insertion_delivered(session_id)
            .map_err(|error| insertion_transition_error("complete", error));
        self.reset_to_idle();
        result
    }

    pub fn fail_insertion(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, DesktopDictationError> {
        self.ensure_inserting(session_id)?;
        let result = self
            .capture
            .fail_dictation_insertion(session_id)
            .map_err(|error| insertion_transition_error("fail", error));
        self.reset_to_idle();
        result
    }

    pub fn cancel(&self, session_id: SessionId) -> Result<SessionSnapshot, DesktopDictationError> {
        let recognizer_to_recycle = {
            let mut slot = self.lock_slot();
            let current = mem::take(&mut *slot);
            match current {
                DictationSlot::Recording(active) if active.session_id == session_id => {
                    Some((active.recognizer_key, active.recognizer))
                }
                DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => None,
                DictationSlot::Recording(active) => {
                    let active_id = active.session_id;
                    *slot = DictationSlot::Recording(active);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::AwaitingInsertion(active_id) => {
                    *slot = DictationSlot::AwaitingInsertion(active_id);
                    return Err(stale_error(session_id, active_id));
                }
                DictationSlot::Finalizing(active_id) => {
                    *slot = DictationSlot::Finalizing(active_id);
                    return Err(busy_error("finalizing"));
                }
                DictationSlot::Inserting(active_id) => {
                    *slot = DictationSlot::Inserting(active_id);
                    return Err(busy_error("inserting"));
                }
                DictationSlot::Preparing => {
                    *slot = DictationSlot::Preparing;
                    return Err(busy_error("preparing"));
                }
                DictationSlot::Idle => {
                    return Err(DesktopDictationError::new(
                        DesktopDictationErrorKind::Busy,
                        "there is no active dictation to cancel",
                    ));
                }
            }
        };

        if let Some((key, recognizer)) = recognizer_to_recycle {
            self.recycle_recognizer(key, recognizer);
        }
        let result = self
            .capture
            .cancel_dictation(session_id)
            .map_err(map_capture_error);
        self.reset_to_idle();
        result
    }

    #[must_use]
    pub fn state_name(&self) -> &'static str {
        self.lock_slot().state_name()
    }

    #[must_use]
    pub fn active_session_id(&self) -> Option<SessionId> {
        self.lock_slot().session_id()
    }

    fn ensure_inserting(&self, session_id: SessionId) -> Result<(), DesktopDictationError> {
        let slot = self.lock_slot();
        match *slot {
            DictationSlot::Inserting(active_id) if active_id == session_id => Ok(()),
            DictationSlot::Inserting(active_id) => Err(stale_error(session_id, active_id)),
            _ => Err(slot_error(&slot, session_id)),
        }
    }

    fn acquire_recognizer(
        &self,
        model_path: &Path,
    ) -> Result<(RecognizerCacheKey, Box<dyn SpeechRecognizer>), RecognitionError> {
        let key = RecognizerCacheKey::for_path(model_path);
        if let Some(cached) = self.lock_recognizer_cache().take()
            && cached.key == key
        {
            return Ok((key, cached.recognizer));
        }
        self.recognizers
            .load(model_path)
            .map(|recognizer| (key, recognizer))
    }

    fn recycle_recognizer(&self, key: RecognizerCacheKey, recognizer: Box<dyn SpeechRecognizer>) {
        *self.lock_recognizer_cache() = Some(CachedRecognizer { key, recognizer });
    }

    fn reset_to_idle(&self) {
        *self.lock_slot() = DictationSlot::Idle;
    }

    fn lock_recognizer_cache(&self) -> MutexGuard<'_, Option<CachedRecognizer>> {
        self.recognizer_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_slot(&self) -> MutexGuard<'_, DictationSlot> {
        self.slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn map_capture_error(error: DesktopCaptureError) -> DesktopDictationError {
    let kind = match error.kind() {
        DesktopCaptureErrorKind::Busy => DesktopDictationErrorKind::Busy,
        DesktopCaptureErrorKind::StaleSession => DesktopDictationErrorKind::StaleSession,
        DesktopCaptureErrorKind::SpeechDetection => DesktopDictationErrorKind::SpeechDetection,
        DesktopCaptureErrorKind::InvalidDevice
        | DesktopCaptureErrorKind::PumpFailed
        | DesktopCaptureErrorKind::WorkerSpawn
        | DesktopCaptureErrorKind::WorkerJoin
        | DesktopCaptureErrorKind::Runtime => DesktopDictationErrorKind::Capture,
    };
    DesktopDictationError::new(kind, error.message().to_owned())
}

fn insertion_transition_error(action: &str, error: DesktopCaptureError) -> DesktopDictationError {
    DesktopDictationError::new(
        DesktopDictationErrorKind::Insertion,
        format!("could not {action} dictation insertion lifecycle: {error}"),
    )
}

fn busy_error(state: &str) -> DesktopDictationError {
    DesktopDictationError::new(
        DesktopDictationErrorKind::Busy,
        format!("dictation service is busy in state {state}"),
    )
}

fn stale_error(supplied: SessionId, active: SessionId) -> DesktopDictationError {
    DesktopDictationError::new(
        DesktopDictationErrorKind::StaleSession,
        format!(
            "dictation session {} is stale; active session is {}",
            supplied.get(),
            active.get()
        ),
    )
}

fn slot_error(slot: &DictationSlot, supplied: SessionId) -> DesktopDictationError {
    match slot.session_id() {
        Some(active) if active != supplied => stale_error(supplied, active),
        _ => busy_error(slot.state_name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blcvoice_asr::{
        AudioFormat, AudioInput, RecognizerCapabilities, TimestampGranularity, Transcription,
    };
    use blcvoice_audio::{
        AudioFailure, AudioSampleFormat, AudioStreamConfig, CaptureStats, InputCaptureFactory,
        InputCaptureSession, InputDeviceDiscovery, InputDiscovery,
    };
    use blcvoice_vad::VadAnalysis;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct AlwaysSpeechDetector;

    impl VoiceActivityDetector for AlwaysSpeechDetector {
        fn backend_name(&self) -> &'static str {
            "test-vad"
        }

        fn analyze_mono(
            &mut self,
            samples: &[f32],
            sample_rate_hz: u32,
            _config: VadConfig,
        ) -> Result<VadAnalysis, blcvoice_vad::VadError> {
            let ranges = if samples.is_empty() {
                Vec::new()
            } else {
                vec![blcvoice_vad::SpeechRange::new(0, samples.len())?]
            };
            VadAnalysis::new(sample_rate_hz, samples.len(), ranges, Some(0.99))
        }
    }

    #[derive(Debug)]
    struct NoSpeechDetector;

    impl VoiceActivityDetector for NoSpeechDetector {
        fn backend_name(&self) -> &'static str {
            "test-vad"
        }

        fn analyze_mono(
            &mut self,
            samples: &[f32],
            sample_rate_hz: u32,
            _config: VadConfig,
        ) -> Result<VadAnalysis, blcvoice_vad::VadError> {
            VadAnalysis::new(sample_rate_hz, samples.len(), Vec::new(), Some(0.01))
        }
    }

    #[derive(Debug)]
    struct FakeDiscovery;

    impl InputDeviceDiscovery for FakeDiscovery {
        fn discover_input_devices(&self) -> InputDiscovery {
            InputDiscovery::default()
        }
    }

    #[derive(Debug)]
    struct FakeCaptureFactory;

    impl InputCaptureFactory for FakeCaptureFactory {
        fn start_input_capture(
            &self,
            _request: &blcvoice_audio::InputCaptureRequest,
        ) -> Result<Box<dyn InputCaptureSession>, AudioFailure> {
            Ok(Box::new(FakeCapture {
                emitted: false,
                config: AudioStreamConfig {
                    channels: 1,
                    sample_rate_hz: 16_000,
                    sample_format: AudioSampleFormat::F32,
                },
            }))
        }
    }

    #[derive(Debug)]
    struct FakeCapture {
        emitted: bool,
        config: AudioStreamConfig,
    }

    impl InputCaptureSession for FakeCapture {
        fn stream_config(&self) -> &AudioStreamConfig {
            &self.config
        }

        fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
            if self.emitted {
                return 0;
            }
            let samples = [0.1_f32, 0.2, 0.3, 0.4];
            output[..samples.len()].copy_from_slice(&samples);
            self.emitted = true;
            samples.len()
        }

        fn stats(&self) -> CaptureStats {
            CaptureStats {
                received_samples: 4,
                ..CaptureStats::default()
            }
        }

        fn pause(&self) -> Result<(), AudioFailure> {
            Ok(())
        }

        fn resume(&self) -> Result<(), AudioFailure> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FakeRecognizerFactory;

    impl RecognizerFactory for FakeRecognizerFactory {
        fn load(&self, _model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
            Ok(Box::new(FakeRecognizer::new()))
        }
    }

    #[derive(Debug)]
    struct CountingRecognizerFactory {
        loads: Arc<AtomicUsize>,
    }

    impl RecognizerFactory for CountingRecognizerFactory {
        fn load(&self, _model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeRecognizer::new()))
        }
    }

    #[derive(Debug)]
    struct FailingRecognizerFactory;

    impl RecognizerFactory for FailingRecognizerFactory {
        fn load(&self, _model_path: &Path) -> Result<Box<dyn SpeechRecognizer>, RecognitionError> {
            Err(RecognitionError::new(
                blcvoice_asr::RecognitionErrorKind::ModelNotFound,
                "missing test model",
            ))
        }
    }

    #[derive(Debug)]
    struct FakeRecognizer {
        capabilities: RecognizerCapabilities,
    }

    impl FakeRecognizer {
        fn new() -> Self {
            Self {
                capabilities: RecognizerCapabilities {
                    required_audio_format: AudioFormat::new(1, 16_000).expect("valid format"),
                    languages: vec!["en".to_owned()],
                    translation_targets: Vec::new(),
                    max_timestamp_granularity: TimestampGranularity::None,
                    supports_language_detection: false,
                    supports_translation: false,
                    supports_streaming: false,
                    supports_cancellation: false,
                    supports_punctuation_control: false,
                    supports_inverse_text_normalization_control: false,
                    max_audio_ms: None,
                },
            }
        }
    }

    impl SpeechRecognizer for FakeRecognizer {
        fn engine_id(&self) -> &'static str {
            "fake"
        }

        fn model_id(&self) -> &str {
            "fake-model"
        }

        fn backend_name(&self) -> &str {
            "cpu"
        }

        fn capabilities(&self) -> &RecognizerCapabilities {
            &self.capabilities
        }

        fn transcribe(
            &mut self,
            _input: AudioInput<'_>,
            _options: &RecognitionOptions,
        ) -> Result<Transcription, RecognitionError> {
            Ok(Transcription {
                text: "hello from BLCVoice".to_owned(),
                detected_language: Some("en".to_owned()),
                ..Transcription::default()
            })
        }
    }

    fn service(recognizers: Arc<dyn RecognizerFactory>) -> DesktopDictationService {
        let capture = Arc::new(DesktopCaptureService::new(
            Arc::new(FakeDiscovery),
            Arc::new(FakeCaptureFactory),
        ));
        DesktopDictationService::new(
            capture,
            recognizers,
            Arc::new(|| Box::new(AlwaysSpeechDetector)),
        )
    }

    fn request() -> DesktopDictationRequest {
        DesktopDictationRequest {
            device_id: AudioDeviceId::new("fake:mic").expect("valid device id"),
            model_path: PathBuf::from("fake-model.bin"),
            recognition: RecognitionOptions::default(),
        }
    }

    #[test]
    fn model_is_prepared_before_capture_starts() {
        let service = service(Arc::new(FailingRecognizerFactory));
        let error = service
            .start(request())
            .expect_err("missing model must block recording");
        assert_eq!(error.kind(), DesktopDictationErrorKind::RecognizerLoad);
        assert_eq!(service.state_name(), "idle");
        assert_eq!(service.capture.current_session(), None);
    }

    #[test]
    fn capture_to_asr_reaches_pending_insertion_with_real_runtime_lifecycle() {
        let service = service(Arc::new(FakeRecognizerFactory));
        let session = service.start(request()).expect("dictation must start");
        let report = service
            .finish(session.id)
            .expect("dictation must transcribe");
        let DesktopDictationFinish::Transcribed(report) = report else {
            panic!("test VAD must report speech");
        };
        assert_eq!(report.engine_id, "fake");
        assert_eq!(report.vad_backend, "test-vad");
        assert_eq!(
            report.transcription.capture.transcription.text,
            "hello from BLCVoice"
        );
        assert_eq!(
            report.transcription.session.state,
            blcvoice_core::SessionState::Inserting
        );
        assert_eq!(service.state_name(), "awaitingInsertion");

        let cancelled = service
            .cancel(session.id)
            .expect("pending insertion dictation must remain cancellable");
        assert_eq!(cancelled.state, blcvoice_core::SessionState::Cancelled);
        assert_eq!(service.state_name(), "idle");
    }

    #[test]
    fn insertion_completion_reaches_completed_and_releases_service() {
        let service = service(Arc::new(FakeRecognizerFactory));
        let session = service.start(request()).expect("dictation must start");
        service
            .finish(session.id)
            .expect("dictation must transcribe");
        service
            .begin_insertion(session.id)
            .expect("insertion must be claimable");
        let completed = service
            .complete_insertion(session.id)
            .expect("insertion must complete");
        assert_eq!(completed.state, blcvoice_core::SessionState::Completed);
        assert_eq!(service.state_name(), "idle");
    }

    #[test]
    fn recognizer_is_reused_after_successful_dictation() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service
            .start(request())
            .expect("first dictation must start");
        service
            .finish(first.id)
            .expect("first dictation must transcribe");
        service
            .begin_insertion(first.id)
            .expect("insertion must begin");
        service
            .complete_insertion(first.id)
            .expect("insertion must complete");

        let second = service
            .start(request())
            .expect("second dictation must start");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        service
            .cancel(second.id)
            .expect("second dictation must cancel");
    }

    #[test]
    fn cancelled_recording_returns_recognizer_to_cache() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service
            .start(request())
            .expect("first dictation must start");
        service
            .cancel(first.id)
            .expect("first dictation must cancel");
        let second = service
            .start(request())
            .expect("second dictation must start");
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        service
            .cancel(second.id)
            .expect("second dictation must cancel");
    }

    #[test]
    fn changing_model_path_replaces_single_entry_cache() {
        let loads = Arc::new(AtomicUsize::new(0));
        let service = service(Arc::new(CountingRecognizerFactory {
            loads: Arc::clone(&loads),
        }));

        let first = service
            .start(request())
            .expect("first dictation must start");
        service
            .cancel(first.id)
            .expect("first dictation must cancel");
        let mut other = request();
        other.model_path = PathBuf::from("other-model.bin");
        let second = service.start(other).expect("other model must start");
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        service
            .cancel(second.id)
            .expect("second dictation must cancel");
    }

    #[test]
    fn stale_finish_cannot_take_active_dictation() {
        let service = service(Arc::new(FakeRecognizerFactory));
        let session = service.start(request()).expect("dictation must start");
        let stale = SessionId::new(session.id.get() + 10);
        let error = service.finish(stale).expect_err("stale finish must fail");
        assert_eq!(error.kind(), DesktopDictationErrorKind::StaleSession);
        assert_eq!(service.active_session_id(), Some(session.id));
        service
            .cancel(session.id)
            .expect("active dictation must cancel");
    }

    #[test]
    fn overlapping_dictation_is_rejected() {
        let service = service(Arc::new(FakeRecognizerFactory));
        let session = service.start(request()).expect("dictation must start");
        let error = service
            .start(request())
            .expect_err("overlapping dictation must be rejected");
        assert_eq!(error.kind(), DesktopDictationErrorKind::Busy);
        service
            .cancel(session.id)
            .expect("active dictation must cancel");
    }

    #[test]
    fn no_speech_skips_asr_and_returns_clean_terminal_outcome() {
        let capture = Arc::new(DesktopCaptureService::new(
            Arc::new(FakeDiscovery),
            Arc::new(FakeCaptureFactory),
        ));
        let service = DesktopDictationService::new(
            capture,
            Arc::new(FakeRecognizerFactory),
            Arc::new(|| Box::new(NoSpeechDetector)),
        );
        let session = service.start(request()).expect("dictation must start");
        let outcome = service
            .finish(session.id)
            .expect("silence must finish cleanly");
        let DesktopDictationFinish::NoSpeech(report) = outcome else {
            panic!("silence must not reach ASR");
        };
        assert_eq!(
            report.terminal_session.state,
            blcvoice_core::SessionState::Cancelled
        );
        assert!(!report.detection.analysis.contains_speech());
        assert_eq!(report.detection.retained_source_frames, 0);
        assert_eq!(service.state_name(), "idle");
    }
}
