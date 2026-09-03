#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;
use std::mem;
use std::sync::{Arc, Mutex, MutexGuard};

use blcvoice_asr::{RecognitionOptions, SpeechRecognizer};
use blcvoice_audio::{AudioFailure, CaptureStats, InputCaptureFactory, InputCaptureRequest};
use blcvoice_core::{
    FailureStage, SessionCoordinator, SessionCoordinatorError, SessionEvent, SessionId,
    SessionSnapshot, SessionState,
};
use blcvoice_dictation::{
    CaptureTranscription, DetectedCaptureTranscription, DictationPipelineError, FinalizedRecording,
    PumpReport, RecordingCollector, SpeechDetectionReport,
};
use blcvoice_vad::{VadConfig, VoiceActivityDetector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeOperation {
    StartRecording,
    PumpRecording,
    FinalizeRecording,
    Transcribe,
}

impl fmt::Display for RuntimeOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::StartRecording => "start recording",
            Self::PumpRecording => "pump recording",
            Self::FinalizeRecording => "finalize recording",
            Self::Transcribe => "transcribe finalized audio",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    Session(SessionCoordinatorError),
    CaptureStart(AudioFailure),
    Pipeline(DictationPipelineError),
    WorkInvalidated {
        session_id: SessionId,
        operation: RuntimeOperation,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => fmt::Display::fmt(error, formatter),
            Self::CaptureStart(error) => {
                write!(formatter, "could not start microphone capture: {error}")
            }
            Self::Pipeline(error) => fmt::Display::fmt(error, formatter),
            Self::WorkInvalidated {
                session_id,
                operation,
            } => write!(
                formatter,
                "dictation session {} can no longer {operation}",
                session_id.get()
            ),
        }
    }
}

impl Error for RuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            Self::CaptureStart(error) => Some(error),
            Self::Pipeline(error) => Some(error),
            Self::WorkInvalidated { .. } => None,
        }
    }
}

impl From<SessionCoordinatorError> for RuntimeError {
    fn from(error: SessionCoordinatorError) -> Self {
        Self::Session(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinalizationReport {
    pub session: SessionSnapshot,
    pub capture_stats: CaptureStats,
    pub source_frames: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTranscription {
    pub session: SessionSnapshot,
    pub capture: CaptureTranscription,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeVadTranscriptionOutcome {
    NoSpeech {
        session: SessionSnapshot,
        detection: SpeechDetectionReport,
    },
    Transcribed {
        transcription: RuntimeTranscription,
        detection: SpeechDetectionReport,
    },
}

enum WorkSlot {
    Empty,
    Arming(SessionId),
    Recording {
        session_id: SessionId,
        collector: RecordingCollector,
    },
    Finalizing(SessionId),
    Finalized {
        session_id: SessionId,
        recording: FinalizedRecording,
    },
    Transcribing(SessionId),
}

impl WorkSlot {
    fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::Empty => None,
            Self::Arming(session_id)
            | Self::Finalizing(session_id)
            | Self::Transcribing(session_id) => Some(*session_id),
            Self::Recording { session_id, .. } | Self::Finalized { session_id, .. } => {
                Some(*session_id)
            }
        }
    }
}

/// Runtime-independent application orchestration for one active BLCVoice dictation.
///
/// The runtime owns ephemeral capture/finalized-audio resources while the domain coordinator owns
/// lifecycle truth. Native capture and ASR implementations remain behind their stable traits.
/// Long-running finalization and recognition are executed without holding the runtime work mutex.
pub struct DictationRuntime {
    coordinator: SessionCoordinator,
    capture_factory: Arc<dyn InputCaptureFactory>,
    work: Mutex<WorkSlot>,
}

impl DictationRuntime {
    #[must_use]
    pub fn new(capture_factory: Arc<dyn InputCaptureFactory>) -> Self {
        Self {
            coordinator: SessionCoordinator::new(),
            capture_factory,
            work: Mutex::new(WorkSlot::Empty),
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<SessionSnapshot> {
        self.coordinator.current()
    }

    #[must_use]
    pub fn can_insert(&self, session_id: SessionId) -> bool {
        self.coordinator.current().is_some_and(|session| {
            session.id == session_id && session.state == SessionState::Inserting
        })
    }

    pub fn start_recording(
        &self,
        request: &InputCaptureRequest,
        max_duration_ms: u32,
    ) -> Result<SessionSnapshot, RuntimeError> {
        let session = self.coordinator.begin()?;
        let session_id = session.id;

        if !self.reserve_arming(session_id) {
            self.best_effort_fail(session_id, FailureStage::Internal);
            return Err(RuntimeError::WorkInvalidated {
                session_id,
                operation: RuntimeOperation::StartRecording,
            });
        }

        let capture = match self.capture_factory.start_input_capture(request) {
            Ok(capture) => capture,
            Err(error) => {
                self.clear_work(session_id);
                self.best_effort_fail(session_id, FailureStage::AudioCapture);
                return Err(RuntimeError::CaptureStart(error));
            }
        };

        let collector = match RecordingCollector::new(capture, max_duration_ms) {
            Ok(collector) => collector,
            Err(error) => {
                self.clear_work(session_id);
                self.best_effort_fail(session_id, capture_failure_stage(&error));
                return Err(RuntimeError::Pipeline(error));
            }
        };

        let transition = match self
            .coordinator
            .transition(session_id, SessionEvent::RecordingStarted)
        {
            Ok(transition) => transition,
            Err(error) => {
                self.clear_work(session_id);
                return Err(RuntimeError::Session(error));
            }
        };

        if !self.store_recording(session_id, collector) {
            self.best_effort_fail(session_id, FailureStage::Internal);
            return Err(RuntimeError::WorkInvalidated {
                session_id,
                operation: RuntimeOperation::StartRecording,
            });
        }

        Ok(transition.snapshot)
    }

    pub fn pump_recording(&self, session_id: SessionId) -> Result<PumpReport, RuntimeError> {
        self.ensure_state(session_id, SessionState::Recording)?;

        let result = {
            let mut work = self.lock_work();
            match &mut *work {
                WorkSlot::Recording {
                    session_id: owned_id,
                    collector,
                } if *owned_id == session_id => collector.pump(),
                _ => {
                    return Err(RuntimeError::WorkInvalidated {
                        session_id,
                        operation: RuntimeOperation::PumpRecording,
                    });
                }
            }
        };

        match result {
            Ok(report) => {
                self.ensure_state(session_id, SessionState::Recording)?;
                Ok(report)
            }
            Err(error) => {
                self.clear_work(session_id);
                self.best_effort_fail(session_id, capture_failure_stage(&error));
                Err(RuntimeError::Pipeline(error))
            }
        }
    }

    pub fn finalize_recording(
        &self,
        session_id: SessionId,
    ) -> Result<FinalizationReport, RuntimeError> {
        self.coordinator
            .transition(session_id, SessionEvent::RecordingStopped)?;

        let collector = self.take_recording_for_finalization(session_id)?;
        let recording = match collector.finalize() {
            Ok(recording) => recording,
            Err(error) => {
                self.clear_work(session_id);
                self.best_effort_fail(session_id, capture_failure_stage(&error));
                return Err(RuntimeError::Pipeline(error));
            }
        };

        let capture_stats = recording.capture_stats();
        let source_frames = recording.source_frames();
        let transition = match self
            .coordinator
            .transition(session_id, SessionEvent::AudioFinalized)
        {
            Ok(transition) => transition,
            Err(error) => {
                self.clear_work(session_id);
                return Err(RuntimeError::Session(error));
            }
        };

        if !self.store_finalized(session_id, recording) {
            return Err(RuntimeError::WorkInvalidated {
                session_id,
                operation: RuntimeOperation::FinalizeRecording,
            });
        }

        Ok(FinalizationReport {
            session: transition.snapshot,
            capture_stats,
            source_frames,
        })
    }

    pub fn transcribe(
        &self,
        session_id: SessionId,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
        requires_transform: bool,
    ) -> Result<RuntimeTranscription, RuntimeError> {
        self.ensure_state(session_id, SessionState::Transcribing)?;
        let recording = self.take_finalized_for_transcription(session_id)?;

        let capture = match recording.transcribe(recognizer, options) {
            Ok(capture) => capture,
            Err(error) => {
                if self.restore_finalized(session_id, recording) {
                    return Err(RuntimeError::Pipeline(error));
                }
                return Err(RuntimeError::WorkInvalidated {
                    session_id,
                    operation: RuntimeOperation::Transcribe,
                });
            }
        };

        let transition = match self.coordinator.transition(
            session_id,
            SessionEvent::TranscriptReady { requires_transform },
        ) {
            Ok(transition) => transition,
            Err(error) => {
                self.clear_work(session_id);
                return Err(RuntimeError::Session(error));
            }
        };

        if !self.clear_transcribing_reservation(session_id) {
            return Err(RuntimeError::WorkInvalidated {
                session_id,
                operation: RuntimeOperation::Transcribe,
            });
        }

        Ok(RuntimeTranscription {
            session: transition.snapshot,
            capture,
        })
    }

    pub fn transcribe_with_vad(
        &self,
        session_id: SessionId,
        detector: &mut dyn VoiceActivityDetector,
        vad_config: VadConfig,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
        requires_transform: bool,
    ) -> Result<RuntimeVadTranscriptionOutcome, RuntimeError> {
        self.ensure_state(session_id, SessionState::Transcribing)?;
        let recording = self.take_finalized_for_transcription(session_id)?;

        let outcome = match recording.transcribe_with_vad(detector, vad_config, recognizer, options)
        {
            Ok(outcome) => outcome,
            Err(error) => {
                if self.restore_finalized(session_id, recording) {
                    return Err(RuntimeError::Pipeline(error));
                }
                return Err(RuntimeError::WorkInvalidated {
                    session_id,
                    operation: RuntimeOperation::Transcribe,
                });
            }
        };

        match outcome {
            DetectedCaptureTranscription::NoSpeech(detection) => {
                if !self.clear_transcribing_reservation(session_id) {
                    return Err(RuntimeError::WorkInvalidated {
                        session_id,
                        operation: RuntimeOperation::Transcribe,
                    });
                }
                let transition = self.coordinator.cancel(session_id)?;
                Ok(RuntimeVadTranscriptionOutcome::NoSpeech {
                    session: transition.snapshot,
                    detection,
                })
            }
            DetectedCaptureTranscription::Transcribed { detection, capture } => {
                let transition = match self.coordinator.transition(
                    session_id,
                    SessionEvent::TranscriptReady { requires_transform },
                ) {
                    Ok(transition) => transition,
                    Err(error) => {
                        self.clear_work(session_id);
                        return Err(RuntimeError::Session(error));
                    }
                };
                if !self.clear_transcribing_reservation(session_id) {
                    return Err(RuntimeError::WorkInvalidated {
                        session_id,
                        operation: RuntimeOperation::Transcribe,
                    });
                }
                Ok(RuntimeVadTranscriptionOutcome::Transcribed {
                    transcription: RuntimeTranscription {
                        session: transition.snapshot,
                        capture,
                    },
                    detection,
                })
            }
        }
    }

    pub fn fail_recognition(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {
        self.fail(session_id, FailureStage::SpeechRecognition)
    }

    pub fn finish_transform(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {
        Ok(self
            .coordinator
            .transition(session_id, SessionEvent::TransformFinished)?
            .snapshot)
    }

    pub fn insertion_delivered(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, RuntimeError> {
        Ok(self
            .coordinator
            .transition(session_id, SessionEvent::InsertionDelivered)?
            .snapshot)
    }

    pub fn fail(
        &self,
        session_id: SessionId,
        stage: FailureStage,
    ) -> Result<SessionSnapshot, RuntimeError> {
        self.clear_work(session_id);
        let transition = self
            .coordinator
            .transition(session_id, SessionEvent::Fail(stage))?;
        Ok(transition.snapshot)
    }

    pub fn cancel(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {
        self.clear_work(session_id);
        let transition = self.coordinator.cancel(session_id)?;
        Ok(transition.snapshot)
    }

    fn ensure_state(
        &self,
        session_id: SessionId,
        expected: SessionState,
    ) -> Result<SessionSnapshot, RuntimeError> {
        let Some(current) = self.coordinator.current() else {
            return Err(RuntimeError::Session(
                SessionCoordinatorError::NoActiveSession,
            ));
        };
        if current.id != session_id {
            return Err(RuntimeError::Session(
                SessionCoordinatorError::StaleSession {
                    supplied: session_id,
                    active: current.id,
                },
            ));
        }
        if current.state != expected {
            return Err(RuntimeError::WorkInvalidated {
                session_id,
                operation: operation_for_state(expected),
            });
        }
        Ok(current)
    }

    fn reserve_arming(&self, session_id: SessionId) -> bool {
        let mut work = self.lock_work();
        if matches!(*work, WorkSlot::Empty) {
            *work = WorkSlot::Arming(session_id);
            true
        } else {
            false
        }
    }

    fn store_recording(&self, session_id: SessionId, collector: RecordingCollector) -> bool {
        let mut work = self.lock_work();
        if matches!(*work, WorkSlot::Arming(owned_id) if owned_id == session_id) {
            *work = WorkSlot::Recording {
                session_id,
                collector,
            };
            true
        } else {
            false
        }
    }

    fn take_recording_for_finalization(
        &self,
        session_id: SessionId,
    ) -> Result<RecordingCollector, RuntimeError> {
        let mut work = self.lock_work();
        let previous = mem::replace(&mut *work, WorkSlot::Finalizing(session_id));
        match previous {
            WorkSlot::Recording {
                session_id: owned_id,
                collector,
            } if owned_id == session_id => Ok(collector),
            other => {
                *work = other;
                Err(RuntimeError::WorkInvalidated {
                    session_id,
                    operation: RuntimeOperation::FinalizeRecording,
                })
            }
        }
    }

    fn store_finalized(&self, session_id: SessionId, recording: FinalizedRecording) -> bool {
        let mut work = self.lock_work();
        if matches!(*work, WorkSlot::Finalizing(owned_id) if owned_id == session_id) {
            *work = WorkSlot::Finalized {
                session_id,
                recording,
            };
            true
        } else {
            false
        }
    }

    fn take_finalized_for_transcription(
        &self,
        session_id: SessionId,
    ) -> Result<FinalizedRecording, RuntimeError> {
        let mut work = self.lock_work();
        let previous = mem::replace(&mut *work, WorkSlot::Transcribing(session_id));
        match previous {
            WorkSlot::Finalized {
                session_id: owned_id,
                recording,
            } if owned_id == session_id => Ok(recording),
            other => {
                *work = other;
                Err(RuntimeError::WorkInvalidated {
                    session_id,
                    operation: RuntimeOperation::Transcribe,
                })
            }
        }
    }

    fn restore_finalized(&self, session_id: SessionId, recording: FinalizedRecording) -> bool {
        let mut work = self.lock_work();
        if matches!(*work, WorkSlot::Transcribing(owned_id) if owned_id == session_id) {
            *work = WorkSlot::Finalized {
                session_id,
                recording,
            };
            true
        } else {
            false
        }
    }

    fn clear_transcribing_reservation(&self, session_id: SessionId) -> bool {
        let mut work = self.lock_work();
        if matches!(*work, WorkSlot::Transcribing(owned_id) if owned_id == session_id) {
            *work = WorkSlot::Empty;
            true
        } else {
            false
        }
    }

    fn clear_work(&self, session_id: SessionId) {
        let mut work = self.lock_work();
        if work.session_id() == Some(session_id) {
            *work = WorkSlot::Empty;
        }
    }

    fn best_effort_fail(&self, session_id: SessionId, stage: FailureStage) {
        let Some(current) = self.coordinator.current() else {
            return;
        };
        if current.id != session_id || current.is_terminal() {
            return;
        }
        let _ = self
            .coordinator
            .transition(session_id, SessionEvent::Fail(stage));
    }

    fn lock_work(&self) -> MutexGuard<'_, WorkSlot> {
        self.work
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn capture_failure_stage(error: &DictationPipelineError) -> FailureStage {
    match error {
        DictationPipelineError::EmptyUtterance | DictationPipelineError::SpeechDetection(_) => {
            FailureStage::SpeechDetection
        }
        DictationPipelineError::InvalidConfiguration(_) => FailureStage::Internal,
        DictationPipelineError::Capture(_)
        | DictationPipelineError::InvalidCaptureRead { .. }
        | DictationPipelineError::CaptureIntegrity { .. }
        | DictationPipelineError::Processing(_) => FailureStage::AudioCapture,
        DictationPipelineError::InvalidAsrAudio(_) | DictationPipelineError::Recognition(_) => {
            FailureStage::Internal
        }
    }
}

const fn operation_for_state(state: SessionState) -> RuntimeOperation {
    match state {
        SessionState::Recording => RuntimeOperation::PumpRecording,
        SessionState::FinalizingAudio => RuntimeOperation::FinalizeRecording,
        SessionState::Transcribing => RuntimeOperation::Transcribe,
        SessionState::Arming
        | SessionState::Transforming
        | SessionState::Inserting
        | SessionState::Completed
        | SessionState::Failed
        | SessionState::Cancelled => RuntimeOperation::StartRecording,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::thread;

    use super::*;
    use blcvoice_asr::{
        AudioFormat as AsrAudioFormat, AudioInput as AsrAudioInput, RecognitionError,
        RecognitionErrorKind, RecognizerCapabilities, TimestampGranularity, Transcription,
    };
    use blcvoice_audio::{
        AudioBackend, AudioDeviceId, AudioFailureKind, AudioSampleFormat, AudioStreamConfig,
        InputCaptureSession,
    };

    #[derive(Clone)]
    struct FakeFactory {
        config: AudioStreamConfig,
        samples: Vec<f32>,
        failure: Option<AudioFailure>,
    }

    impl FakeFactory {
        fn with_frames(frames: usize) -> Self {
            Self {
                config: stereo_48k(),
                samples: stereo_samples(frames),
                failure: None,
            }
        }

        fn empty() -> Self {
            Self {
                config: stereo_48k(),
                samples: Vec::new(),
                failure: None,
            }
        }

        fn failing() -> Self {
            Self {
                config: stereo_48k(),
                samples: Vec::new(),
                failure: Some(AudioFailure {
                    backend: Some(AudioBackend::Wasapi),
                    device_id: Some(test_device_id()),
                    kind: AudioFailureKind::PermissionDenied,
                    message: "microphone permission denied".to_owned(),
                }),
            }
        }
    }

    impl InputCaptureFactory for FakeFactory {
        fn start_input_capture(
            &self,
            _request: &InputCaptureRequest,
        ) -> Result<Box<dyn InputCaptureSession>, AudioFailure> {
            if let Some(error) = self.failure.clone() {
                return Err(error);
            }
            Ok(Box::new(FakeCapture {
                config: self.config.clone(),
                samples: self.samples.clone(),
                position: 0,
            }))
        }
    }

    struct FakeCapture {
        config: AudioStreamConfig,
        samples: Vec<f32>,
        position: usize,
    }

    impl InputCaptureSession for FakeCapture {
        fn stream_config(&self) -> &AudioStreamConfig {
            &self.config
        }

        fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
            let remaining = &self.samples[self.position..];
            if remaining.is_empty() {
                return 0;
            }
            let copied = remaining.len().min(output.len());
            output[..copied].copy_from_slice(&remaining[..copied]);
            self.position += copied;
            copied
        }

        fn stats(&self) -> CaptureStats {
            CaptureStats {
                received_samples: self.samples.len() as u64,
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

    struct FakeRecognizer {
        capabilities: RecognizerCapabilities,
        fail_next: bool,
    }

    impl FakeRecognizer {
        fn success() -> Self {
            Self {
                capabilities: recognizer_capabilities(),
                fail_next: false,
            }
        }

        fn fail_once() -> Self {
            Self {
                capabilities: recognizer_capabilities(),
                fail_next: true,
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
            "test"
        }

        fn capabilities(&self) -> &RecognizerCapabilities {
            &self.capabilities
        }

        fn transcribe(
            &mut self,
            _input: AsrAudioInput<'_>,
            _options: &RecognitionOptions,
        ) -> Result<Transcription, RecognitionError> {
            if self.fail_next {
                self.fail_next = false;
                return Err(RecognitionError::new(
                    RecognitionErrorKind::BackendUnavailable,
                    "temporary backend outage",
                ));
            }
            Ok(Transcription {
                text: "hello runtime".to_owned(),
                ..Transcription::default()
            })
        }
    }

    struct BlockingRecognizer {
        capabilities: RecognizerCapabilities,
        entered: Sender<()>,
        release: Receiver<()>,
    }

    impl SpeechRecognizer for BlockingRecognizer {
        fn engine_id(&self) -> &'static str {
            "blocking-fake"
        }

        fn model_id(&self) -> &str {
            "fake-model"
        }

        fn backend_name(&self) -> &str {
            "test"
        }

        fn capabilities(&self) -> &RecognizerCapabilities {
            &self.capabilities
        }

        fn transcribe(
            &mut self,
            _input: AsrAudioInput<'_>,
            _options: &RecognitionOptions,
        ) -> Result<Transcription, RecognitionError> {
            self.entered.send(()).expect("test receiver must exist");
            self.release.recv().expect("test release must arrive");
            Ok(Transcription {
                text: "late transcript".to_owned(),
                ..Transcription::default()
            })
        }
    }

    fn runtime(factory: FakeFactory) -> Arc<DictationRuntime> {
        Arc::new(DictationRuntime::new(Arc::new(factory)))
    }

    fn request() -> InputCaptureRequest {
        InputCaptureRequest {
            device_id: test_device_id(),
            buffer: Default::default(),
        }
    }

    fn test_device_id() -> AudioDeviceId {
        AudioDeviceId::new("wasapi:test-microphone").expect("valid test device id")
    }

    fn stereo_48k() -> AudioStreamConfig {
        AudioStreamConfig {
            channels: 2,
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::F32,
        }
    }

    fn stereo_samples(frames: usize) -> Vec<f32> {
        let mut samples = Vec::with_capacity(frames * 2);
        for _ in 0..frames {
            samples.extend_from_slice(&[0.2, 0.2]);
        }
        samples
    }

    fn recognizer_capabilities() -> RecognizerCapabilities {
        RecognizerCapabilities {
            required_audio_format: AsrAudioFormat::new(1, 16_000).expect("valid test ASR format"),
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
        }
    }

    fn advance_to_transcribing(runtime: &DictationRuntime) -> SessionId {
        let session = runtime
            .start_recording(&request(), 1_000)
            .expect("recording must start");
        runtime
            .pump_recording(session.id)
            .expect("recording must pump");
        let finalized = runtime
            .finalize_recording(session.id)
            .expect("recording must finalize");
        assert_eq!(finalized.session.state, SessionState::Transcribing);
        session.id
    }

    #[test]
    fn orchestrates_through_explicit_insertion_delivery() {
        let runtime = runtime(FakeFactory::with_frames(4_800));
        let session = runtime
            .start_recording(&request(), 1_000)
            .expect("recording must start");
        assert_eq!(session.state, SessionState::Recording);

        let pump = runtime
            .pump_recording(session.id)
            .expect("recording must pump");
        assert_eq!(pump.frames_read, 4_800);

        let finalized = runtime
            .finalize_recording(session.id)
            .expect("recording must finalize");
        assert_eq!(finalized.session.state, SessionState::Transcribing);
        assert_eq!(finalized.source_frames, 4_800);

        let mut recognizer = FakeRecognizer::success();
        let transcript = runtime
            .transcribe(
                session.id,
                &mut recognizer,
                &RecognitionOptions::default(),
                false,
            )
            .expect("recognition must succeed");
        assert_eq!(transcript.capture.transcription.text, "hello runtime");
        assert_eq!(transcript.session.state, SessionState::Inserting);
        assert!(runtime.can_insert(session.id));

        let completed = runtime
            .insertion_delivered(session.id)
            .expect("delivery must complete the session");
        assert_eq!(completed.state, SessionState::Completed);
        assert!(!runtime.can_insert(session.id));

        let next = runtime
            .start_recording(&request(), 1_000)
            .expect("a terminal session must allow the next recording");
        assert_eq!(next.id.get(), session.id.get() + 1);
        runtime
            .cancel(next.id)
            .expect("cleanup cancel must succeed");
    }

    #[test]
    fn transform_gate_must_finish_before_insertion() {
        let runtime = runtime(FakeFactory::with_frames(4_800));
        let session_id = advance_to_transcribing(&runtime);
        let mut recognizer = FakeRecognizer::success();

        let transcript = runtime
            .transcribe(
                session_id,
                &mut recognizer,
                &RecognitionOptions::default(),
                true,
            )
            .expect("recognition must succeed");
        assert_eq!(transcript.session.state, SessionState::Transforming);
        assert!(!runtime.can_insert(session_id));

        let inserting = runtime
            .finish_transform(session_id)
            .expect("transform completion must advance insertion");
        assert_eq!(inserting.state, SessionState::Inserting);
        assert!(runtime.can_insert(session_id));
        runtime
            .cancel(session_id)
            .expect("cleanup cancel must succeed");
    }

    #[test]
    fn recognition_failure_restores_finalized_audio_for_retry() {
        let runtime = runtime(FakeFactory::with_frames(4_800));
        let session_id = advance_to_transcribing(&runtime);
        let mut recognizer = FakeRecognizer::fail_once();

        let error = runtime
            .transcribe(
                session_id,
                &mut recognizer,
                &RecognitionOptions::default(),
                false,
            )
            .expect_err("first recognition attempt must fail");
        assert!(matches!(
            error,
            RuntimeError::Pipeline(DictationPipelineError::Recognition(_))
        ));
        assert_eq!(
            runtime
                .current()
                .expect("session must remain visible")
                .state,
            SessionState::Transcribing
        );

        let retried = runtime
            .transcribe(
                session_id,
                &mut recognizer,
                &RecognitionOptions::default(),
                false,
            )
            .expect("same finalized audio must be retryable");
        assert_eq!(retried.capture.transcription.text, "hello runtime");
        runtime
            .cancel(session_id)
            .expect("cleanup cancel must succeed");
    }

    #[test]
    fn capture_start_failure_marks_audio_capture_stage() {
        let runtime = runtime(FakeFactory::failing());
        let error = runtime
            .start_recording(&request(), 1_000)
            .expect_err("capture start must fail");

        assert!(matches!(error, RuntimeError::CaptureStart(_)));
        let session = runtime
            .current()
            .expect("failed session must remain visible");
        assert_eq!(session.state, SessionState::Failed);
        assert_eq!(session.failure_stage, Some(FailureStage::AudioCapture));
    }

    #[test]
    fn empty_finalization_marks_speech_detection_stage() {
        let runtime = runtime(FakeFactory::empty());
        let session = runtime
            .start_recording(&request(), 1_000)
            .expect("recording must start");
        let error = runtime
            .finalize_recording(session.id)
            .expect_err("empty recording must fail finalization");

        assert!(matches!(
            error,
            RuntimeError::Pipeline(DictationPipelineError::EmptyUtterance)
        ));
        let failed = runtime
            .current()
            .expect("failed session must remain visible");
        assert_eq!(failed.state, SessionState::Failed);
        assert_eq!(failed.failure_stage, Some(FailureStage::SpeechDetection));
    }

    #[test]
    fn cancelled_old_recognition_cannot_clear_or_mutate_new_recording() {
        let runtime = runtime(FakeFactory::with_frames(4_800));
        let first_id = advance_to_transcribing(&runtime);
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        let worker_runtime = Arc::clone(&runtime);

        let worker = thread::spawn(move || {
            let mut recognizer = BlockingRecognizer {
                capabilities: recognizer_capabilities(),
                entered: entered_tx,
                release: release_rx,
            };
            worker_runtime.transcribe(
                first_id,
                &mut recognizer,
                &RecognitionOptions::default(),
                false,
            )
        });

        entered_rx
            .recv()
            .expect("recognizer must enter before cancellation");
        runtime.cancel(first_id).expect("first session must cancel");
        let second = runtime
            .start_recording(&request(), 1_000)
            .expect("new recording must start while old ASR unwinds");

        release_tx.send(()).expect("recognizer must be released");
        let late_result = worker.join().expect("worker thread must not panic");
        assert!(matches!(
            late_result,
            Err(RuntimeError::Session(
                SessionCoordinatorError::StaleSession { supplied, active }
            )) if supplied == first_id && active == second.id
        ));

        assert_eq!(
            runtime.current().expect("new session must remain current"),
            second
        );
        runtime
            .pump_recording(second.id)
            .expect("late worker must not clear the new recording collector");
        runtime
            .cancel(second.id)
            .expect("cleanup cancel must succeed");
    }

    #[test]
    fn runtime_is_safe_to_share_across_worker_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DictationRuntime>();
    }
}
