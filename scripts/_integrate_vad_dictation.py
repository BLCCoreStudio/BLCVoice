from pathlib import Path


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        raise SystemExit(f"marker not found in {path}: {old[:80]!r}")
    target.write_text(text.replace(old, new, count))


replace(
    "crates/blcvoice-dictation/Cargo.toml",
    'blcvoice-audio-processing = { path = "../blcvoice-audio-processing" }\n',
    'blcvoice-audio-processing = { path = "../blcvoice-audio-processing" }\nblcvoice-vad = { path = "../blcvoice-vad" }\n',
)
replace(
    "crates/blcvoice-runtime/Cargo.toml",
    'blcvoice-dictation = { path = "../blcvoice-dictation" }\n',
    'blcvoice-dictation = { path = "../blcvoice-dictation" }\nblcvoice-vad = { path = "../blcvoice-vad" }\n',
)
replace(
    "apps/desktop/src-tauri/Cargo.toml",
    'blcvoice-shortcuts = { path = "../../../crates/blcvoice-shortcuts" }\n',
    'blcvoice-shortcuts = { path = "../../../crates/blcvoice-shortcuts" }\nblcvoice-vad = { path = "../../../crates/blcvoice-vad" }\nblcvoice-vad-silero = { path = "../../../crates/blcvoice-vad-silero" }\n',
)

# Worker-side finalized-audio VAD seam.
path = Path("crates/blcvoice-dictation/src/lib.rs")
text = path.read_text()
text = text.replace(
    'use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\n',
    'use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\nuse blcvoice_vad::{VadConfig, VadError, VoiceActivityDetector};\n',
    1,
)
method_marker = '''    #[must_use]\n    pub const fn capture_stats(&self) -> CaptureStats {\n        self.capture_stats\n    }\n\n'''
if method_marker not in text:
    raise SystemExit("FinalizedRecording method marker not found")
# Insert a shared preprocessing helper before public transcription methods.
helper = '''    fn preprocess_for_recognizer(\n        &self,\n        recognizer: &dyn SpeechRecognizer,\n    ) -> Result<(AsrAudioFormat, blcvoice_audio_processing::ProcessedAudio), DictationPipelineError> {\n        let asr_format = recognizer.capabilities().required_audio_format;\n        let processing_target = processing_format(asr_format)?;\n        let mut preprocessor = AudioPreprocessor::new(self.utterance.format(), processing_target)\n            .map_err(DictationPipelineError::Processing)?;\n        let processed = preprocessor\n            .process_utterance(self.utterance.as_interleaved())\n            .map_err(DictationPipelineError::Processing)?;\n        Ok((asr_format, processed))\n    }\n\n'''
text = text.replace(method_marker, method_marker + helper, 1)
old_transcribe = '''    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.\n    ///\n    /// The recording is borrowed rather than consumed so a caller may retry a transient recognition\n    /// failure without reopening the microphone or duplicating the source audio buffer.\n    pub fn transcribe(\n        &self,\n        recognizer: &mut dyn SpeechRecognizer,\n        options: &RecognitionOptions,\n    ) -> Result<CaptureTranscription, DictationPipelineError> {\n        let asr_format = recognizer.capabilities().required_audio_format;\n        let processing_target = processing_format(asr_format)?;\n        let mut preprocessor = AudioPreprocessor::new(self.utterance.format(), processing_target)\n            .map_err(DictationPipelineError::Processing)?;\n        let source_frames = self.utterance.frames();\n        let processed = preprocessor\n            .process_utterance(self.utterance.as_interleaved())\n            .map_err(DictationPipelineError::Processing)?;\n        let asr_frames = processed.frames();\n        let input = AsrAudioInput::new(processed.samples(), asr_format)\n            .map_err(DictationPipelineError::InvalidAsrAudio)?;\n        let transcription = recognizer\n            .transcribe(input, options)\n            .map_err(DictationPipelineError::Recognition)?;\n\n        Ok(CaptureTranscription {\n            transcription,\n            capture_stats: self.capture_stats,\n            source_frames,\n            asr_frames,\n        })\n    }\n'''
new_transcribe = '''    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.\n    ///\n    /// The recording is borrowed rather than consumed so a caller may retry a transient recognition\n    /// failure without reopening the microphone or duplicating the source audio buffer.\n    pub fn transcribe(\n        &self,\n        recognizer: &mut dyn SpeechRecognizer,\n        options: &RecognitionOptions,\n    ) -> Result<CaptureTranscription, DictationPipelineError> {\n        let source_frames = self.utterance.frames();\n        let (asr_format, processed) = self.preprocess_for_recognizer(recognizer)?;\n        let asr_frames = processed.frames();\n        let input = AsrAudioInput::new(processed.samples(), asr_format)\n            .map_err(DictationPipelineError::InvalidAsrAudio)?;\n        let transcription = recognizer\n            .transcribe(input, options)\n            .map_err(DictationPipelineError::Recognition)?;\n\n        Ok(CaptureTranscription {\n            transcription,\n            capture_stats: self.capture_stats,\n            source_frames,\n            asr_frames,\n        })\n    }\n\n    /// Run VAD on recognizer-format mono audio, preserve internal pauses, trim only the outer\n    /// non-speech envelope, and skip ASR entirely when no speech is detected.\n    pub fn transcribe_with_vad(\n        &self,\n        recognizer: &mut dyn SpeechRecognizer,\n        options: &RecognitionOptions,\n        detector: &mut dyn VoiceActivityDetector,\n        vad_config: VadConfig,\n    ) -> Result<CaptureTranscription, DictationPipelineError> {\n        let source_frames = self.utterance.frames();\n        let (asr_format, processed) = self.preprocess_for_recognizer(recognizer)?;\n        if asr_format.channels() != 1 {\n            return Err(DictationPipelineError::InvalidConfiguration(\n                "voice activity detection requires mono recognizer audio",\n            ));\n        }\n        let analysis = detector\n            .analyze_mono(processed.samples(), asr_format.sample_rate_hz(), vad_config)\n            .map_err(DictationPipelineError::SpeechDetection)?;\n        let speech = analysis\n            .speech_envelope()\n            .ok_or(DictationPipelineError::NoSpeechDetected)?;\n        let speech_samples = &processed.samples()[speech.start_sample..speech.end_sample];\n        let input = AsrAudioInput::new(speech_samples, asr_format)\n            .map_err(DictationPipelineError::InvalidAsrAudio)?;\n        let asr_frames = input.frames();\n        let transcription = recognizer\n            .transcribe(input, options)\n            .map_err(DictationPipelineError::Recognition)?;\n\n        Ok(CaptureTranscription {\n            transcription,\n            capture_stats: self.capture_stats,\n            source_frames,\n            asr_frames,\n        })\n    }\n'''
if old_transcribe not in text:
    raise SystemExit("transcribe implementation marker not found")
text = text.replace(old_transcribe, new_transcribe, 1)
text = text.replace(
    '    EmptyUtterance,\n    Processing(ProcessingError),\n',
    '    EmptyUtterance,\n    NoSpeechDetected,\n    SpeechDetection(VadError),\n    Processing(ProcessingError),\n',
    1,
)
text = text.replace(
    '            Self::EmptyUtterance => {\n                formatter.write_str("captured utterance contains no audio frames")\n            }\n            Self::Processing(error) => write!(formatter, "audio preprocessing failed: {error}"),\n',
    '            Self::EmptyUtterance => {\n                formatter.write_str("captured utterance contains no audio frames")\n            }\n            Self::NoSpeechDetected => formatter.write_str("captured utterance contains no detected speech"),\n            Self::SpeechDetection(error) => write!(formatter, "speech detection failed: {error}"),\n            Self::Processing(error) => write!(formatter, "audio preprocessing failed: {error}"),\n',
    1,
)
text = text.replace(
    '            Self::Capture(error) => Some(error),\n            Self::Processing(error) => Some(error),\n',
    '            Self::Capture(error) => Some(error),\n            Self::SpeechDetection(error) => Some(error),\n            Self::Processing(error) => Some(error),\n',
    1,
)
text = text.replace(
    '            | Self::CaptureIntegrity { .. }\n            | Self::EmptyUtterance => None,\n',
    '            | Self::CaptureIntegrity { .. }\n            | Self::EmptyUtterance\n            | Self::NoSpeechDetected => None,\n',
    1,
)
# Add deterministic VAD seam tests inside the existing test module.
insert_at = text.rfind("\n}")
if insert_at < 0:
    raise SystemExit("dictation tests module ending not found")
vad_tests = r'''

    struct FakeVad {
        speech_range: Option<blcvoice_vad::SpeechRange>,
        calls: usize,
    }

    impl VoiceActivityDetector for FakeVad {
        fn backend_name(&self) -> &'static str {
            "fake-vad"
        }

        fn analyze_mono(
            &mut self,
            samples: &[f32],
            sample_rate_hz: u32,
            _config: VadConfig,
        ) -> Result<blcvoice_vad::VadAnalysis, VadError> {
            self.calls += 1;
            let ranges = self.speech_range.into_iter().collect();
            blcvoice_vad::VadAnalysis::new(sample_rate_hz, samples.len(), ranges, Some(0.9))
        }
    }

    #[test]
    fn vad_trims_only_the_outer_speech_envelope_before_asr() {
        let capture = Box::new(FakeCapture::normal(stereo_48k(), stereo_samples(4_800)));
        let collector = RecordingCollector::new(capture, 1_000).unwrap();
        let finalized = collector.finalize().unwrap();
        let mut recognizer = FakeRecognizer::mono_16k();
        let mut detector = FakeVad {
            speech_range: Some(blcvoice_vad::SpeechRange::new(100, 1_000).unwrap()),
            calls: 0,
        };

        let result = finalized
            .transcribe_with_vad(
                &mut recognizer,
                &RecognitionOptions::default(),
                &mut detector,
                VadConfig::default(),
            )
            .unwrap();

        assert_eq!(detector.calls, 1);
        assert_eq!(recognizer.calls, 1);
        assert_eq!(recognizer.seen_frames, 900);
        assert_eq!(result.asr_frames, 900);
        assert_eq!(result.source_frames, 4_800);
    }

    #[test]
    fn vad_no_speech_skips_recognizer_entirely() {
        let capture = Box::new(FakeCapture::normal(stereo_48k(), stereo_samples(4_800)));
        let collector = RecordingCollector::new(capture, 1_000).unwrap();
        let finalized = collector.finalize().unwrap();
        let mut recognizer = FakeRecognizer::mono_16k();
        let mut detector = FakeVad {
            speech_range: None,
            calls: 0,
        };

        let error = finalized
            .transcribe_with_vad(
                &mut recognizer,
                &RecognitionOptions::default(),
                &mut detector,
                VadConfig::default(),
            )
            .unwrap_err();

        assert!(matches!(error, DictationPipelineError::NoSpeechDetected));
        assert_eq!(detector.calls, 1);
        assert_eq!(recognizer.calls, 0);
    }
'''
text = text[:insert_at] + vad_tests + text[insert_at:]
path.write_text(text)

# Runtime: expose VAD transcription while keeping finalized-audio restore semantics in one helper.
path = Path("crates/blcvoice-runtime/src/lib.rs")
text = path.read_text()
text = text.replace(
    'use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\n',
    'use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\n',
    1,
)
start = text.index('    pub fn transcribe(\n')
end = text.index('    pub fn fail_recognition', start)
new_runtime = r'''    pub fn transcribe(
        &self,
        session_id: SessionId,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
        requires_transform: bool,
    ) -> Result<RuntimeTranscription, RuntimeError> {
        self.transcribe_finalized(session_id, requires_transform, |recording| {
            recording.transcribe(recognizer, options)
        })
    }

    pub fn transcribe_with_vad(
        &self,
        session_id: SessionId,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
        detector: &mut dyn VoiceActivityDetector,
        vad_config: VadConfig,
        requires_transform: bool,
    ) -> Result<RuntimeTranscription, RuntimeError> {
        self.transcribe_finalized(session_id, requires_transform, |recording| {
            recording.transcribe_with_vad(recognizer, options, detector, vad_config)
        })
    }

    fn transcribe_finalized<F>(
        &self,
        session_id: SessionId,
        requires_transform: bool,
        transcribe: F,
    ) -> Result<RuntimeTranscription, RuntimeError>
    where
        F: FnOnce(&FinalizedRecording) -> Result<CaptureTranscription, DictationPipelineError>,
    {
        self.ensure_state(session_id, SessionState::Transcribing)?;
        let recording = self.take_finalized_for_transcription(session_id)?;

        let capture = match transcribe(&recording) {
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

'''
text = text[:start] + new_runtime + text[end:]
text = text.replace(
    '    pub fn fail_recognition(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {\n        self.fail(session_id, FailureStage::SpeechRecognition)\n    }\n',
    '    pub fn fail_recognition(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {\n        self.fail(session_id, FailureStage::SpeechRecognition)\n    }\n\n    pub fn fail_speech_detection(\n        &self,\n        session_id: SessionId,\n    ) -> Result<SessionSnapshot, RuntimeError> {\n        self.fail(session_id, FailureStage::SpeechDetection)\n    }\n',
    1,
)
text = text.replace(
    '        DictationPipelineError::EmptyUtterance => FailureStage::SpeechDetection,\n',
    '        DictationPipelineError::EmptyUtterance\n        | DictationPipelineError::NoSpeechDetected\n        | DictationPipelineError::SpeechDetection(_) => FailureStage::SpeechDetection,\n',
    1,
)
path.write_text(text)

# Desktop capture bridge: preserve speech-detection error kind and expose VAD path.
path = Path("apps/desktop/src-tauri/src/capture.rs")
text = path.read_text()
text = text.replace(
    'use blcvoice_core::{FailureStage, SessionId, SessionSnapshot, SessionState};\nuse blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\n',
    'use blcvoice_core::{FailureStage, SessionId, SessionSnapshot, SessionState};\nuse blcvoice_dictation::DictationPipelineError;\nuse blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\n',
    1,
)
text = text.replace(
    '    Runtime,\n}\n',
    '    Runtime,\n    SpeechDetection,\n}\n',
    1,
)
text = text.replace(
    'impl From<RuntimeError> for DesktopCaptureError {\n    fn from(error: RuntimeError) -> Self {\n        Self::new(DesktopCaptureErrorKind::Runtime, error.to_string())\n    }\n}\n',
    '''impl From<RuntimeError> for DesktopCaptureError {\n    fn from(error: RuntimeError) -> Self {\n        let kind = match &error {\n            RuntimeError::Pipeline(\n                DictationPipelineError::NoSpeechDetected\n                | DictationPipelineError::SpeechDetection(_),\n            ) => DesktopCaptureErrorKind::SpeechDetection,\n            _ => DesktopCaptureErrorKind::Runtime,\n        };\n        Self::new(kind, error.to_string())\n    }\n}\n''',
    1,
)
marker = '''    pub fn transcribe_dictation(\n        &self,\n        session_id: SessionId,\n        recognizer: &mut dyn SpeechRecognizer,\n        options: &RecognitionOptions,\n    ) -> Result<RuntimeTranscription, DesktopCaptureError> {\n        self.runtime\n            .transcribe(session_id, recognizer, options, false)\n            .map_err(DesktopCaptureError::from)\n    }\n\n'''
if marker not in text:
    raise SystemExit("desktop transcribe bridge marker not found")
addition = marker + '''    pub fn transcribe_dictation_with_vad(\n        &self,\n        session_id: SessionId,\n        recognizer: &mut dyn SpeechRecognizer,\n        options: &RecognitionOptions,\n        detector: &mut dyn VoiceActivityDetector,\n        vad_config: VadConfig,\n    ) -> Result<RuntimeTranscription, DesktopCaptureError> {\n        self.runtime\n            .transcribe_with_vad(\n                session_id,\n                recognizer,\n                options,\n                detector,\n                vad_config,\n                false,\n            )\n            .map_err(DesktopCaptureError::from)\n    }\n\n'''
text = text.replace(marker, addition, 1)
text = text.replace(
    '    pub fn fail_dictation_recognition(\n        &self,\n        session_id: SessionId,\n    ) -> Result<SessionSnapshot, DesktopCaptureError> {\n        self.runtime\n            .fail_recognition(session_id)\n            .map_err(DesktopCaptureError::from)\n    }\n',
    '    pub fn fail_dictation_recognition(\n        &self,\n        session_id: SessionId,\n    ) -> Result<SessionSnapshot, DesktopCaptureError> {\n        self.runtime\n            .fail_recognition(session_id)\n            .map_err(DesktopCaptureError::from)\n    }\n\n    pub fn fail_dictation_speech_detection(\n        &self,\n        session_id: SessionId,\n    ) -> Result<SessionSnapshot, DesktopCaptureError> {\n        self.runtime\n            .fail_speech_detection(session_id)\n            .map_err(DesktopCaptureError::from)\n    }\n',
    1,
)
path.write_text(text)

# Production desktop service: enable Silero only in production constructor; unit-test constructor keeps legacy path.
path = Path("apps/desktop/src-tauri/src/dictation.rs")
text = path.read_text()
text = text.replace(
    'use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\n',
    'use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\nuse blcvoice_vad::VadConfig;\nuse blcvoice_vad_silero::SileroVoiceActivityDetector;\n',
    1,
)
text = text.replace(
    '    Transcription,\n    Insertion,\n}\n',
    '    Transcription,\n    SpeechDetection,\n    Insertion,\n}\n',
    1,
)
text = text.replace(
    '    recognizer_cache: Mutex<Option<CachedRecognizer>>,\n    slot: Mutex<DictationSlot>,\n',
    '    recognizer_cache: Mutex<Option<CachedRecognizer>>,\n    vad_enabled: bool,\n    slot: Mutex<DictationSlot>,\n',
    1,
)
old_ctor = '''    #[must_use]\n    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {\n        Self::new(capture, Arc::new(TranscribeRecognizerFactory))\n    }\n\n    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {\n        Self {\n            capture,\n            recognizers,\n            recognizer_cache: Mutex::new(None),\n            slot: Mutex::new(DictationSlot::Idle),\n        }\n    }\n'''
new_ctor = '''    #[must_use]\n    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {\n        Self::with_vad(capture, Arc::new(TranscribeRecognizerFactory), true)\n    }\n\n    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {\n        Self::with_vad(capture, recognizers, false)\n    }\n\n    fn with_vad(\n        capture: Arc<DesktopCaptureService>,\n        recognizers: Arc<dyn RecognizerFactory>,\n        vad_enabled: bool,\n    ) -> Self {\n        Self {\n            capture,\n            recognizers,\n            recognizer_cache: Mutex::new(None),\n            vad_enabled,\n            slot: Mutex::new(DictationSlot::Idle),\n        }\n    }\n'''
if old_ctor not in text:
    raise SystemExit("desktop dictation constructor marker not found")
text = text.replace(old_ctor, new_ctor, 1)
old_call = '''        let transcription_result = self.capture.transcribe_dictation(\n            session_id,\n            active.recognizer.as_mut(),\n            &active.recognition,\n        );\n'''
new_call = '''        let transcription_result = if self.vad_enabled {\n            let mut detector = SileroVoiceActivityDetector::new();\n            self.capture.transcribe_dictation_with_vad(\n                session_id,\n                active.recognizer.as_mut(),\n                &active.recognition,\n                &mut detector,\n                VadConfig::default(),\n            )\n        } else {\n            self.capture.transcribe_dictation(\n                session_id,\n                active.recognizer.as_mut(),\n                &active.recognition,\n            )\n        };\n'''
if old_call not in text:
    raise SystemExit("desktop transcription call marker not found")
text = text.replace(old_call, new_call, 1)
old_error = '''            Err(error) => {\n                let _ = self.capture.fail_dictation_recognition(session_id);\n                self.reset_to_idle();\n                return Err(DesktopDictationError::new(\n                    DesktopDictationErrorKind::Transcription,\n                    format!("dictation transcription failed: {error}"),\n                ));\n            }\n'''
new_error = '''            Err(error) => {\n                let speech_detection = error.kind() == DesktopCaptureErrorKind::SpeechDetection;\n                if speech_detection {\n                    let _ = self.capture.fail_dictation_speech_detection(session_id);\n                } else {\n                    let _ = self.capture.fail_dictation_recognition(session_id);\n                }\n                self.reset_to_idle();\n                let kind = if speech_detection {\n                    DesktopDictationErrorKind::SpeechDetection\n                } else {\n                    DesktopDictationErrorKind::Transcription\n                };\n                return Err(DesktopDictationError::new(\n                    kind,\n                    format!("dictation transcription failed: {error}"),\n                ));\n            }\n'''
if old_error not in text:
    raise SystemExit("desktop transcription error marker not found")
text = text.replace(old_error, new_error, 1)
text = text.replace(
    '        DesktopCaptureErrorKind::InvalidDevice\n        | DesktopCaptureErrorKind::PumpFailed\n',
    '        DesktopCaptureErrorKind::SpeechDetection => DesktopDictationErrorKind::SpeechDetection,\n        DesktopCaptureErrorKind::InvalidDevice\n        | DesktopCaptureErrorKind::PumpFailed\n',
    1,
)
path.write_text(text)

# IPC: preserve typed speech-detection/no-speech code.
path = Path("apps/desktop/src-tauri/src/ipc.rs")
text = path.read_text()
text = text.replace(
    '            DesktopCaptureErrorKind::Runtime => "dictation_runtime_failed",\n',
    '            DesktopCaptureErrorKind::Runtime => "dictation_runtime_failed",\n            DesktopCaptureErrorKind::SpeechDetection => "speech_detection_failed",\n',
    1,
)
text = text.replace(
    '            DesktopDictationErrorKind::Transcription => "dictation_transcription_failed",\n',
    '            DesktopDictationErrorKind::Transcription => "dictation_transcription_failed",\n            DesktopDictationErrorKind::SpeechDetection => "no_speech_detected",\n',
    1,
)
path.write_text(text)
