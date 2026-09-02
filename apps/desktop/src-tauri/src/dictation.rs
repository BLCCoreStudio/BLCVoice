use std::error::Error;
use std::fmt;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use blcvoice_asr::{RecognitionError, RecognitionOptions, SpeechRecognizer};
use blcvoice_asr_transcribe::{TranscribeRecognizer, TranscribeRecognizerConfig};
use blcvoice_audio::AudioDeviceId;
use blcvoice_core::{SessionId, SessionSnapshot};
use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};

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
    Transcription,
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
    pub engine_id: String,
    pub model_id: String,
    pub backend_name: String,
}

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

struct ActiveDictation {
    session_id: SessionId,
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
}

impl DictationSlot {
    fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::Recording(active) => Some(active.session_id),
            Self::Finalizing(session_id) | Self::AwaitingInsertion(session_id) => Some(*session_id),
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
        }
    }
}

pub struct DesktopDictationService {
    capture: Arc<DesktopCaptureService>,
    recognizers: Arc<dyn RecognizerFactory>,
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
        Self::new(capture, Arc::new(TranscribeRecognizerFactory))
    }

    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {
        Self {
            capture,
            recognizers,
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

        let recognizer = match self.recognizers.load(&request.model_path) {
            Ok(recognizer) => recognizer,
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
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let mut slot = self.lock_slot();
        if !matches!(*slot, DictationSlot::Preparing) {
            drop(slot);
            let _ = self.capture.cancel_dictation(session.id);
            self.reset_to_idle();
            return Err(DesktopDictationError::new(
                DesktopDictationErrorKind::Busy,
                "dictation preparation was invalidated before recording started",
            ));
        }
        *slot = DictationSlot::Recording(ActiveDictation {
            session_id: session.id,
            recognizer,
            recognition: request.recognition,
        });

        Ok(session)
    }

    pub fn finish(
        &self,
        session_id: SessionId,
    ) -> Result<DesktopDictationReport, DesktopDictationError> {
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
                self.reset_to_idle();
                return Err(map_capture_error(error));
            }
        };

        let engine_id = active.recognizer.engine_id().to_owned();
        let model_id = active.recognizer.model_id().to_owned();
        let backend_name = active.recognizer.backend_name().to_owned();
        let transcription = match self.capture.transcribe_dictation(
            session_id,
            active.recognizer.as_mut(),
            &active.recognition,
        ) {
            Ok(transcription) => transcription,
            Err(error) => {
                let _ = self.capture.fail_dictation_recognition(session_id);
                self.reset_to_idle();
                return Err(DesktopDictationError::new(
                    DesktopDictationErrorKind::Transcription,
                    format!("dictation transcription failed: {error}"),
                ));
            }
        };

        let mut slot = self.lock_slot();
        *slot = DictationSlot::AwaitingInsertion(session_id);

        Ok(DesktopDictationReport {
            finalized,
            transcription,
            engine_id,
            model_id,
            backend_name,
        })
    }

    pub fn cancel(&self, session_id: SessionId) -> Result<SessionSnapshot, DesktopDictationError> {
        {
            let mut slot = self.lock_slot();
            let current = mem::take(&mut *slot);
            match current {
                DictationSlot::Recording(active) if active.session_id == session_id => {}
                DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => {}
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
        }

        let result = self
            .capture
            .cancel_dictation(session_id)
            .map_err(map_capture_error);
        self.reset_to_idle();
        result
    }

    pub fn insertion_delivered(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, DesktopDictationError> {
        {
            let slot = self.lock_slot();
            match *slot {
                DictationSlot::AwaitingInsertion(active_id) if active_id == session_id => {}
                DictationSlot::AwaitingInsertion(active_id) => {
                    return Err(stale_error(session_id, active_id));
                }
                _ => return Err(busy_error(slot.state_name())),
            }
        }

        let result = self
            .capture
            .dictation_insertion_delivered(session_id)
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

    fn reset_to_idle(&self) {
        *self.lock_slot() = DictationSlot::Idle;
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
        DesktopCaptureErrorKind::InvalidDevice
        | DesktopCaptureErrorKind::PumpFailed
        | DesktopCaptureErrorKind::WorkerSpawn
        | DesktopCaptureErrorKind::WorkerJoin
        | DesktopCaptureErrorKind::Runtime => DesktopDictationErrorKind::Capture,
    };
    DesktopDictationError::new(kind, error.message().to_owned())
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
        DesktopDictationService::new(capture, recognizers)
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

        assert_eq!(report.engine_id, "fake");
        assert_eq!(report.model_id, "fake-model");
        assert_eq!(
            report.transcription.capture.transcription.text,
            "hello from BLCVoice"
        );
        assert_eq!(
            report.transcription.session.state,
            blcvoice_core::SessionState::Inserting
        );
        assert_eq!(service.state_name(), "awaitingInsertion");

        let completed = service
            .insertion_delivered(session.id)
            .expect("insertion acknowledgement must complete lifecycle");
        assert_eq!(completed.state, blcvoice_core::SessionState::Completed);
        assert_eq!(service.state_name(), "idle");
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
}
