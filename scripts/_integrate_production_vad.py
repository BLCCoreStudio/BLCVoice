from pathlib import Path


def replace_once(path, old, new):
    file = Path(path)
    source = file.read_text()
    if old not in source:
        raise SystemExit(f"marker not found in {path}: {old[:120]!r}")
    file.write_text(source.replace(old, new, 1))


# Dependency edges.
replace_once(
    "crates/blcvoice-dictation/Cargo.toml",
    'blcvoice-audio-processing = { path = "../blcvoice-audio-processing" }\n',
    'blcvoice-audio-processing = { path = "../blcvoice-audio-processing" }\nblcvoice-vad = { path = "../blcvoice-vad" }\n',
)
replace_once(
    "crates/blcvoice-runtime/Cargo.toml",
    'blcvoice-dictation = { path = "../blcvoice-dictation" }\n',
    'blcvoice-dictation = { path = "../blcvoice-dictation" }\nblcvoice-vad = { path = "../blcvoice-vad" }\n',
)
replace_once(
    "apps/desktop/src-tauri/Cargo.toml",
    'blcvoice-shortcuts = { path = "../../../crates/blcvoice-shortcuts" }\n',
    'blcvoice-shortcuts = { path = "../../../crates/blcvoice-shortcuts" }\nblcvoice-vad = { path = "../../../crates/blcvoice-vad" }\nblcvoice-vad-silero = { path = "../../../crates/blcvoice-vad-silero" }\n',
)

# FinalizedRecording: detect speech on mono source-rate audio and transcribe only the outer speech envelope.
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    'use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\n',
    'use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VadError, VoiceActivityDetector};\n',
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    'impl FinalizedRecording {\n',
    '''#[derive(Debug, Clone, PartialEq)]
pub struct SpeechDetectionReport {
    pub analysis: VadAnalysis,
    pub captured_source_frames: usize,
    pub retained_source_frames: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectedCaptureTranscription {
    NoSpeech(SpeechDetectionReport),
    Transcribed {
        detection: SpeechDetectionReport,
        capture: CaptureTranscription,
    },
}

impl FinalizedRecording {
''',
)
marker = '    /// Preprocess this finalized utterance to the recognizer\'s required format and run ASR.\n'
vad_method = '''    /// Detect speech before recognition and transcribe only the outer speech envelope.
    ///
    /// VAD runs on a mono view at the native source sample rate, so detected sample indices map
    /// exactly to source frames. Only leading/trailing silence is removed; internal pauses remain.
    pub fn transcribe_with_vad(
        &self,
        detector: &mut dyn VoiceActivityDetector,
        vad_config: VadConfig,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
    ) -> Result<DetectedCaptureTranscription, DictationPipelineError> {
        let source_format = self.utterance.format();
        let source_frames = self.utterance.frames();
        let mono_format = ProcessingAudioFormat::new(1, source_format.sample_rate_hz())
            .map_err(DictationPipelineError::Processing)?;
        let mut vad_preprocessor = AudioPreprocessor::new(source_format, mono_format)
            .map_err(DictationPipelineError::Processing)?;
        let mono = vad_preprocessor
            .process_utterance(self.utterance.as_interleaved())
            .map_err(DictationPipelineError::Processing)?;
        if mono.frames() != source_frames {
            return Err(DictationPipelineError::InvalidConfiguration(
                "same-rate VAD preprocessing changed the source frame count",
            ));
        }
        let analysis = detector
            .analyze_mono(mono.samples(), mono_format.sample_rate_hz(), vad_config)
            .map_err(DictationPipelineError::SpeechDetection)?;

        let Some(envelope) = analysis.speech_envelope() else {
            return Ok(DetectedCaptureTranscription::NoSpeech(
                SpeechDetectionReport {
                    analysis,
                    captured_source_frames: source_frames,
                    retained_source_frames: 0,
                },
            ));
        };

        let channels = usize::from(source_format.channels());
        let start_sample = envelope
            .start_sample
            .checked_mul(channels)
            .ok_or(DictationPipelineError::InvalidConfiguration(
                "VAD speech range overflowed the source buffer",
            ))?;
        let end_sample = envelope
            .end_sample
            .checked_mul(channels)
            .ok_or(DictationPipelineError::InvalidConfiguration(
                "VAD speech range overflowed the source buffer",
            ))?;
        let source = self
            .utterance
            .as_interleaved()
            .get(start_sample..end_sample)
            .ok_or(DictationPipelineError::InvalidConfiguration(
                "VAD speech range fell outside the source buffer",
            ))?;

        let asr_format = recognizer.capabilities().required_audio_format;
        let processing_target = processing_format(asr_format)?;
        let mut preprocessor = AudioPreprocessor::new(source_format, processing_target)
            .map_err(DictationPipelineError::Processing)?;
        let processed = preprocessor
            .process_utterance(source)
            .map_err(DictationPipelineError::Processing)?;
        let asr_frames = processed.frames();
        let input = AsrAudioInput::new(processed.samples(), asr_format)
            .map_err(DictationPipelineError::InvalidAsrAudio)?;
        let transcription = recognizer
            .transcribe(input, options)
            .map_err(DictationPipelineError::Recognition)?;
        let retained_source_frames = envelope.sample_len();
        let capture = CaptureTranscription {
            transcription,
            capture_stats: self.capture_stats,
            source_frames: retained_source_frames,
            asr_frames,
        };

        Ok(DetectedCaptureTranscription::Transcribed {
            detection: SpeechDetectionReport {
                analysis,
                captured_source_frames: source_frames,
                retained_source_frames,
            },
            capture,
        })
    }

'''
replace_once("crates/blcvoice-dictation/src/lib.rs", marker, vad_method + marker)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    '    EmptyUtterance,\n    Processing(ProcessingError),\n',
    '    EmptyUtterance,\n    SpeechDetection(VadError),\n    Processing(ProcessingError),\n',
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    '            Self::EmptyUtterance => {\n                formatter.write_str("captured utterance contains no audio frames")\n            }\n            Self::Processing(error) => write!(formatter, "audio preprocessing failed: {error}"),\n',
    '            Self::EmptyUtterance => {\n                formatter.write_str("captured utterance contains no audio frames")\n            }\n            Self::SpeechDetection(error) => write!(formatter, "speech detection failed: {error}"),\n            Self::Processing(error) => write!(formatter, "audio preprocessing failed: {error}"),\n',
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    '            Self::Capture(error) => Some(error),\n            Self::Processing(error) => Some(error),\n',
    '            Self::Capture(error) => Some(error),\n            Self::SpeechDetection(error) => Some(error),\n            Self::Processing(error) => Some(error),\n',
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    '            | Self::CaptureIntegrity { .. }\n            | Self::EmptyUtterance => None,\n',
    '            | Self::CaptureIntegrity { .. }\n            | Self::EmptyUtterance => None,\n',
)

# Runtime: preserve the existing transcribe API and add a VAD-aware production path.
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    'use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\n',
    'use blcvoice_dictation::{\n    CaptureTranscription, DetectedCaptureTranscription, DictationPipelineError, FinalizedRecording,\n    PumpReport, RecordingCollector, SpeechDetectionReport,\n};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\n',
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    '''#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeTranscription {
    pub session: SessionSnapshot,
    pub capture: CaptureTranscription,
}
''',
    '''#[derive(Debug, Clone, PartialEq)]
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
''',
)
insert_marker = '    pub fn fail_recognition(&self, session_id: SessionId) -> Result<SessionSnapshot, RuntimeError> {\n'
runtime_method = '''    pub fn transcribe_with_vad(
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

        let outcome = match recording.transcribe_with_vad(detector, vad_config, recognizer, options) {
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

'''
replace_once("crates/blcvoice-runtime/src/lib.rs", insert_marker, runtime_method + insert_marker)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    '        DictationPipelineError::EmptyUtterance => FailureStage::SpeechDetection,\n',
    '        DictationPipelineError::EmptyUtterance | DictationPipelineError::SpeechDetection(_) => {\n            FailureStage::SpeechDetection\n        }\n',
)

# Desktop capture: keep speech-detection failure typed through the host boundary.
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    'use blcvoice_core::{FailureStage, SessionId, SessionSnapshot, SessionState};\nuse blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\n',
    'use blcvoice_core::{FailureStage, SessionId, SessionSnapshot, SessionState};\nuse blcvoice_dictation::DictationPipelineError;\nuse blcvoice_runtime::{\n    DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription,\n    RuntimeVadTranscriptionOutcome,\n};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\n',
)
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    '    Runtime,\n}\n',
    '    Runtime,\n    SpeechDetection,\n}\n',
)
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    '''impl From<RuntimeError> for DesktopCaptureError {
    fn from(error: RuntimeError) -> Self {
        Self::new(DesktopCaptureErrorKind::Runtime, error.to_string())
    }
}
''',
    '''impl From<RuntimeError> for DesktopCaptureError {
    fn from(error: RuntimeError) -> Self {
        let kind = if matches!(
            &error,
            RuntimeError::Pipeline(DictationPipelineError::SpeechDetection(_))
        ) {
            DesktopCaptureErrorKind::SpeechDetection
        } else {
            DesktopCaptureErrorKind::Runtime
        };
        Self::new(kind, error.to_string())
    }
}
''',
)
transcribe_marker = '    pub fn fail_dictation_recognition(\n'
capture_method = '''    pub fn transcribe_dictation_with_vad(
        &self,
        session_id: SessionId,
        detector: &mut dyn VoiceActivityDetector,
        vad_config: VadConfig,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
    ) -> Result<RuntimeVadTranscriptionOutcome, DesktopCaptureError> {
        self.runtime
            .transcribe_with_vad(
                session_id,
                detector,
                vad_config,
                recognizer,
                options,
                false,
            )
            .map_err(DesktopCaptureError::from)
    }

    pub fn fail_dictation_speech_detection(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, DesktopCaptureError> {
        self.runtime
            .fail(session_id, FailureStage::SpeechDetection)
            .map_err(DesktopCaptureError::from)
    }

'''
replace_once("apps/desktop/src-tauri/src/capture.rs", transcribe_marker, capture_method + transcribe_marker)

# Desktop dictation: dependency-injected VAD factory for deterministic tests, Silero in production.
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    'use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\n',
    'use blcvoice_dictation::SpeechDetectionReport;\nuse blcvoice_runtime::{FinalizationReport, RuntimeTranscription, RuntimeVadTranscriptionOutcome};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VoiceActivityDetector};\nuse blcvoice_vad_silero::SileroVoiceActivityDetector;\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '    Transcription,\n    Insertion,\n',
    '    SpeechDetection,\n    Transcription,\n    Insertion,\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '''#[derive(Debug, Clone, PartialEq)]
pub struct DesktopDictationReport {
    pub finalized: FinalizationReport,
    pub transcription: RuntimeTranscription,
    pub engine_id: String,
    pub model_id: String,
    pub backend_name: String,
}
''',
    '''#[derive(Debug, Clone, PartialEq)]
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
''',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '    recognizers: Arc<dyn RecognizerFactory>,\n    recognizer_cache: Mutex<Option<CachedRecognizer>>,\n',
    '    recognizers: Arc<dyn RecognizerFactory>,\n    vad_factory: Arc<VadFactory>,\n    recognizer_cache: Mutex<Option<CachedRecognizer>>,\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '''    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {
        Self::new(capture, Arc::new(TranscribeRecognizerFactory))
    }

    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {
        Self {
            capture,
            recognizers,
            recognizer_cache: Mutex::new(None),
            slot: Mutex::new(DictationSlot::Idle),
        }
    }
''',
    '''    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {
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
''',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '    ) -> Result<DesktopDictationReport, DesktopDictationError> {\n',
    '    ) -> Result<DesktopDictationFinish, DesktopDictationError> {\n',
)
old_finish_block = '''        let engine_id = active.recognizer.engine_id().to_owned();
        let model_id = active.recognizer.model_id().to_owned();
        let backend_name = active.recognizer.backend_name().to_owned();
        let transcription_result = self.capture.transcribe_dictation(
            session_id,
            active.recognizer.as_mut(),
            &active.recognition,
        );
        self.recycle_recognizer(active.recognizer_key, active.recognizer);
        let transcription = match transcription_result {
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

        *self.lock_slot() = DictationSlot::AwaitingInsertion(session_id);

        Ok(DesktopDictationReport {
            finalized,
            transcription,
            engine_id,
            model_id,
            backend_name,
        })
'''
new_finish_block = '''        let engine_id = active.recognizer.engine_id().to_owned();
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
                Ok(DesktopDictationFinish::Transcribed(DesktopDictationReport {
                    finalized,
                    transcription,
                    detection,
                    vad_backend,
                    engine_id,
                    model_id,
                    backend_name,
                }))
            }
        }
'''
replace_once("apps/desktop/src-tauri/src/dictation.rs", old_finish_block, new_finish_block)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '        DesktopCaptureErrorKind::StaleSession => DesktopDictationErrorKind::StaleSession,\n',
    '        DesktopCaptureErrorKind::StaleSession => DesktopDictationErrorKind::StaleSession,\n        DesktopCaptureErrorKind::SpeechDetection => DesktopDictationErrorKind::SpeechDetection,\n',
)

# Test VAD: existing fake audio is deliberately tiny, so make test intent deterministic.
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '    use std::sync::atomic::{AtomicUsize, Ordering};\n',
    '    use std::sync::atomic::{AtomicUsize, Ordering};\n\n    #[derive(Debug)]\n    struct AlwaysSpeechDetector;\n\n    impl VoiceActivityDetector for AlwaysSpeechDetector {\n        fn backend_name(&self) -> &\'static str {\n            "test-vad"\n        }\n\n        fn analyze_mono(\n            &mut self,\n            samples: &[f32],\n            sample_rate_hz: u32,\n            _config: VadConfig,\n        ) -> Result<VadAnalysis, blcvoice_vad::VadError> {\n            let ranges = if samples.is_empty() {\n                Vec::new()\n            } else {\n                vec![blcvoice_vad::SpeechRange::new(0, samples.len())?]\n            };\n            VadAnalysis::new(sample_rate_hz, samples.len(), ranges, Some(0.99))\n        }\n    }\n\n    #[derive(Debug)]\n    struct NoSpeechDetector;\n\n    impl VoiceActivityDetector for NoSpeechDetector {\n        fn backend_name(&self) -> &\'static str {\n            "test-vad"\n        }\n\n        fn analyze_mono(\n            &mut self,\n            samples: &[f32],\n            sample_rate_hz: u32,\n            _config: VadConfig,\n        ) -> Result<VadAnalysis, blcvoice_vad::VadError> {\n            VadAnalysis::new(sample_rate_hz, samples.len(), Vec::new(), Some(0.01))\n        }\n    }\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '        DesktopDictationService::new(capture, recognizers)\n',
    '        DesktopDictationService::new(\n            capture,\n            recognizers,\n            Arc::new(|| Box::new(AlwaysSpeechDetector)),\n        )\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '''        let report = service
            .finish(session.id)
            .expect("dictation must transcribe");
        assert_eq!(report.engine_id, "fake");
        assert_eq!(
            report.transcription.capture.transcription.text,
            "hello from BLCVoice"
        );
        assert_eq!(
            report.transcription.session.state,
            blcvoice_core::SessionState::Inserting
        );
''',
    '''        let report = service
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
''',
)
# Add a no-speech service helper/test before the final module brace.
dictation_path = Path("apps/desktop/src-tauri/src/dictation.rs")
dictation_source = dictation_path.read_text()
insert = '''

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
        let outcome = service.finish(session.id).expect("silence must finish cleanly");
        let DesktopDictationFinish::NoSpeech(report) = outcome else {
            panic!("silence must not reach ASR");
        };
        assert_eq!(report.terminal_session.state, blcvoice_core::SessionState::Cancelled);
        assert!(!report.detection.analysis.contains_speech());
        assert_eq!(report.detection.retained_source_frames, 0);
        assert_eq!(service.state_name(), "idle");
    }
'''
if not dictation_source.rstrip().endswith("}"):
    raise SystemExit("dictation test module closing brace not found")
dictation_path.write_text(dictation_source.rstrip()[:-1] + insert + "}\n")

# IPC: clean no-speech DTO, VAD diagnostics, no insertion side effect.
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '    DesktopDictationError, DesktopDictationErrorKind, DesktopDictationReport,\n    DesktopDictationRequest, DesktopDictationService,\n',
    '    DesktopDictationError, DesktopDictationErrorKind, DesktopDictationFinish,\n    DesktopDictationReport, DesktopDictationRequest, DesktopDictationService, DesktopNoSpeechReport,\n',
)
old_finish_session = '''        let report = self
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
'''
new_finish_session = '''        let outcome = self
            .dictation
            .finish(session_id)
            .map_err(CommandErrorDto::from)?;
        let DesktopDictationFinish::Transcribed(report) = outcome else {
            let DesktopDictationFinish::NoSpeech(report) = outcome else {
                unreachable!();
            };
            return Ok(DictationReportDto::no_speech(report));
        };
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
'''
replace_once("apps/desktop/src-tauri/src/ipc.rs", old_finish_session, new_finish_session)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '            DesktopCaptureErrorKind::StaleSession => "stale_session",\n',
    '            DesktopCaptureErrorKind::StaleSession => "stale_session",\n            DesktopCaptureErrorKind::SpeechDetection => "speech_detection_failed",\n',
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '            DesktopDictationErrorKind::RecognizerLoad => "recognizer_load_failed",\n            DesktopDictationErrorKind::Capture => "dictation_capture_failed",\n',
    '            DesktopDictationErrorKind::RecognizerLoad => "recognizer_load_failed",\n            DesktopDictationErrorKind::Capture => "dictation_capture_failed",\n            DesktopDictationErrorKind::SpeechDetection => "speech_detection_failed",\n',
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '''    insertion_backend: String,
    submitted_utf8_bytes: usize,
    semantic_delivery_verified: bool,
''',
    '''    speech_detected: bool,
    vad_backend: String,
    vad_max_speech_probability: Option<f32>,
    speech_source_frames: usize,
    insertion_backend: Option<String>,
    submitted_utf8_bytes: usize,
    semantic_delivery_verified: bool,
''',
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '''    pub(crate) fn insertion_backend(&self) -> &str {
        &self.insertion_backend
    }
''',
    '''    pub(crate) fn insertion_backend(&self) -> Option<&str> {
        self.insertion_backend.as_deref()
    }

    pub(crate) const fn speech_detected(&self) -> bool {
        self.speech_detected
    }
''',
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    '''            source_frames: capture.source_frames,
            asr_frames: capture.asr_frames,
            capture_stats: CaptureStatsDto::from(capture.capture_stats),
            insertion_backend: receipt.backend().to_string(),
            submitted_utf8_bytes: receipt.submitted_utf8_bytes(),
            semantic_delivery_verified: receipt.semantic_delivery_verified(),
        }
    }
''',
    '''            source_frames: report.finalized.source_frames,
            asr_frames: capture.asr_frames,
            capture_stats: CaptureStatsDto::from(capture.capture_stats),
            speech_detected: true,
            vad_backend: report.vad_backend,
            vad_max_speech_probability: report.detection.analysis.max_speech_probability,
            speech_source_frames: report.detection.retained_source_frames,
            insertion_backend: Some(receipt.backend().to_string()),
            submitted_utf8_bytes: receipt.submitted_utf8_bytes(),
            semantic_delivery_verified: receipt.semantic_delivery_verified(),
        }
    }

    fn no_speech(report: DesktopNoSpeechReport) -> Self {
        Self {
            session_id: report.terminal_session.id.get(),
            state: session_state_name(report.terminal_session.state),
            text: String::new(),
            raw_text: None,
            detected_language: None,
            engine_id: report.engine_id,
            model_id: report.model_id,
            backend_name: report.backend_name,
            source_frames: report.finalized.source_frames,
            asr_frames: 0,
            capture_stats: CaptureStatsDto::from(report.finalized.capture_stats),
            speech_detected: false,
            vad_backend: report.vad_backend,
            vad_max_speech_probability: report.detection.analysis.max_speech_probability,
            speech_source_frames: 0,
            insertion_backend: None,
            submitted_utf8_bytes: 0,
            semantic_delivery_verified: false,
        }
    }
''',
)

# Shortcut lifecycle: no speech is a clean no-op, not a failed dictation.
replace_once(
    "apps/desktop/src-tauri/src/coordinator.rs",
    '''    fn completed(session_id: SessionId, report: &DictationReportDto) -> Self {
        Self {
            source: "shortcut",
            state: "completed",
            session_id: Some(session_id.get()),
            text: Some(report.text().to_owned()),
            insertion_backend: Some(report.insertion_backend().to_owned()),
            error_code: None,
            message: None,
            recoverable_text: None,
        }
    }
''',
    '''    fn completed(session_id: SessionId, report: &DictationReportDto) -> Self {
        if !report.speech_detected() {
            return Self {
                source: "shortcut",
                state: "noSpeech",
                session_id: Some(session_id.get()),
                text: None,
                insertion_backend: None,
                error_code: None,
                message: Some("No speech was detected; nothing was inserted.".to_owned()),
                recoverable_text: None,
            };
        }
        Self {
            source: "shortcut",
            state: "completed",
            session_id: Some(session_id.get()),
            text: Some(report.text().to_owned()),
            insertion_backend: report.insertion_backend().map(str::to_owned),
            error_code: None,
            message: None,
            recoverable_text: None,
        }
    }
''',
)

# UI: explicit no-speech result for button and global-shortcut flows.
replace_once(
    "apps/desktop/ui/app.js",
    '''    const report = await invoke("dictation_finish", { sessionId });
    state.dictationSessionId = null;
    const language = report.detectedLanguage ? report.detectedLanguage.toUpperCase() : "auto";
    showTranscript(report.text, `${language} · ${report.insertionBackend}`);
''',
    '''    const report = await invoke("dictation_finish", { sessionId });
    state.dictationSessionId = null;
    if (!report.speechDetected) {
      showTranscript("", "");
      setPill(elements.dictationState, "Ready", "idle");
      elements.dictationMessage.textContent = "No speech was detected; nothing was inserted.";
      clearDictationError();
      return;
    }
    const language = report.detectedLanguage ? report.detectedLanguage.toUpperCase() : "auto";
    showTranscript(report.text, `${language} · ${report.insertionBackend}`);
''',
)
replace_once(
    "apps/desktop/ui/app.js",
    '    case "failed":\n',
    '''    case "noSpeech":
      state.shortcutSessionActive = false;
      state.dictationBusy = false;
      state.dictationSessionId = null;
      clearDictationError();
      showTranscript("", "");
      setPill(elements.dictationState, "Ready", "idle");
      elements.dictationMessage.textContent = payload.message || "No speech was detected; nothing was inserted.";
      break;
    case "failed":
''',
)

# ADR for the production policy.
Path("docs/adr/0025-production-vad-policy.md").write_text('''# ADR 0025: Production VAD policy\n\n## Status\n\nAccepted.\n\n## Decision\n\nProduction dictation runs engine-agnostic Silero VAD after capture finalization and before ASR.\n\n- VAD analyzes a mono view at the native capture sample rate.\n- If no speech is detected, the session terminates cleanly as a no-op: ASR and text insertion are skipped.\n- If speech is detected, only leading and trailing silence outside the outer speech envelope is removed before ASR.\n- Silence between speech regions is preserved; BLCVoice does not concatenate VAD segments.\n- Speech-detection backend failures are reported as `SpeechDetection`, not mislabeled as recognition failures.\n- The existing maximum dictation duration remains the hard safety bound. Streaming automatic endpointing is a separate policy layer.\n\n## Rationale\n\nThis prevents silent clips from reaching an ASR engine and reduces unnecessary context without erasing natural pauses. Keeping VAD outside transcribe.cpp preserves the engine-agnostic ASR contract and supports model families that do not expose native VAD.\n\n## Non-goals\n\nThis decision does not add meeting segmentation, diarization, continuous listening, or automatic stop-on-silence.\n''')
