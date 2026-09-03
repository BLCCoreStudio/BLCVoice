from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"marker not found in {path}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1))


# Audio buffer: allow zero-allocation outer-envelope trimming in source-frame coordinates.
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "    UnalignedInput {\n        channels: u16,\n        samples: usize,\n    },\n    UtteranceTooLong {",
    "    UnalignedInput {\n        channels: u16,\n        samples: usize,\n    },\n    InvalidFrameRange {\n        start_frame: usize,\n        end_frame: usize,\n        available_frames: usize,\n    },\n    UtteranceTooLong {",
)
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "            Self::UnalignedInput { channels, samples } => write!(\n                formatter,\n                \"{samples} interleaved samples do not form complete {channels}-channel frames\"\n            ),\n            Self::UtteranceTooLong {",
    "            Self::UnalignedInput { channels, samples } => write!(\n                formatter,\n                \"{samples} interleaved samples do not form complete {channels}-channel frames\"\n            ),\n            Self::InvalidFrameRange {\n                start_frame,\n                end_frame,\n                available_frames,\n            } => write!(\n                formatter,\n                \"invalid audio frame range {start_frame}..{end_frame} for {available_frames} available frames\"\n            ),\n            Self::UtteranceTooLong {",
)
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "    /// Clear the utterance while retaining allocated capacity for the next dictation.\n    pub fn clear(&mut self) {",
    "    /// Retain one contiguous source-frame range without allocating a replacement buffer.\n    ///\n    /// VAD uses this only for the outer speech envelope, so internal pauses remain intact.\n    pub fn retain_frame_range(\n        &mut self,\n        start_frame: usize,\n        end_frame: usize,\n    ) -> Result<(), ProcessingError> {\n        let available_frames = self.frames();\n        if start_frame >= end_frame || end_frame > available_frames {\n            return Err(ProcessingError::InvalidFrameRange {\n                start_frame,\n                end_frame,\n                available_frames,\n            });\n        }\n        let channels = usize::from(self.format.channels);\n        let start_sample = start_frame\n            .checked_mul(channels)\n            .ok_or(ProcessingError::BufferSizeOverflow)?;\n        let end_sample = end_frame\n            .checked_mul(channels)\n            .ok_or(ProcessingError::BufferSizeOverflow)?;\n        let retained_samples = end_sample\n            .checked_sub(start_sample)\n            .ok_or(ProcessingError::BufferSizeOverflow)?;\n        self.samples.copy_within(start_sample..end_sample, 0);\n        self.samples.truncate(retained_samples);\n        Ok(())\n    }\n\n    /// Clear the utterance while retaining allocated capacity for the next dictation.\n    pub fn clear(&mut self) {",
)
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "    #[test]\n    fn rejects_invalid_signal_shapes() {",
    "    #[test]\n    fn retains_complete_interleaved_frames_for_a_speech_envelope() {\n        let mut utterance = UtteranceBuffer::new(format(2, 16_000), 8).unwrap();\n        utterance\n            .push_interleaved(&[1.0, 10.0, 2.0, 20.0, 3.0, 30.0, 4.0, 40.0])\n            .unwrap();\n        utterance.retain_frame_range(1, 3).unwrap();\n        assert_eq!(utterance.frames(), 2);\n        assert_eq!(utterance.as_interleaved(), &[2.0, 20.0, 3.0, 30.0]);\n    }\n\n    #[test]\n    fn rejects_invalid_signal_shapes() {",
)

# Dictation layer: analyze a finalized native utterance through the VAD contract and trim only the outer envelope.
replace_once(
    "crates/blcvoice-dictation/Cargo.toml",
    "blcvoice-audio-processing = { path = \"../blcvoice-audio-processing\" }\n",
    "blcvoice-audio-processing = { path = \"../blcvoice-audio-processing\" }\nblcvoice-vad = { path = \"../blcvoice-vad\" }\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\n",
    "use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VadError, VoiceActivityDetector};\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.\n",
    "    /// Analyze speech activity and, when speech exists, retain only the outer speech envelope.\n    ///\n    /// The detector receives mono audio at the source sample rate. Because no resampling occurs,\n    /// detector sample indices are identical to native source-frame indices. Internal pauses are\n    /// deliberately preserved.\n    pub fn analyze_and_trim_speech(\n        &mut self,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<VadAnalysis, DictationPipelineError> {\n        let source = self.utterance.format();\n        let mono = ProcessingAudioFormat::new(1, source.sample_rate_hz())\n            .map_err(DictationPipelineError::Processing)?;\n        let mut preprocessor = AudioPreprocessor::new(source, mono)\n            .map_err(DictationPipelineError::Processing)?;\n        let processed = preprocessor\n            .process_utterance(self.utterance.as_interleaved())\n            .map_err(DictationPipelineError::Processing)?;\n        let analysis = detector\n            .analyze_mono(processed.samples(), source.sample_rate_hz(), config)\n            .map_err(DictationPipelineError::VoiceActivity)?;\n        if let Some(envelope) = analysis.speech_envelope() {\n            self.utterance\n                .retain_frame_range(envelope.start_sample, envelope.end_sample)\n                .map_err(DictationPipelineError::Processing)?;\n        }\n        Ok(analysis)\n    }\n\n    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    Processing(ProcessingError),\n    InvalidAsrAudio(AudioInputError),",
    "    Processing(ProcessingError),\n    VoiceActivity(VadError),\n    InvalidAsrAudio(AudioInputError),",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "            Self::Processing(error) => write!(formatter, \"audio preprocessing failed: {error}\"),\n            Self::InvalidAsrAudio(error) => {",
    "            Self::Processing(error) => write!(formatter, \"audio preprocessing failed: {error}\"),\n            Self::VoiceActivity(error) => write!(formatter, \"voice activity detection failed: {error}\"),\n            Self::InvalidAsrAudio(error) => {",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "            Self::Processing(error) => Some(error),\n            Self::InvalidAsrAudio(error) => Some(error),",
    "            Self::Processing(error) => Some(error),\n            Self::VoiceActivity(error) => Some(error),\n            Self::InvalidAsrAudio(error) => Some(error),",
)

# Runtime: temporarily claim finalized work for VAD, then restore the possibly trimmed recording for ASR.
replace_once(
    "crates/blcvoice-runtime/Cargo.toml",
    "blcvoice-dictation = { path = \"../blcvoice-dictation\" }\n",
    "blcvoice-dictation = { path = \"../blcvoice-dictation\" }\nblcvoice-vad = { path = \"../blcvoice-vad\" }\n",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\n",
    "use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VoiceActivityDetector};\n",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "    FinalizeRecording,\n    Transcribe,\n}",
    "    FinalizeRecording,\n    VoiceActivity,\n    Transcribe,\n}",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "            Self::FinalizeRecording => \"finalize recording\",\n            Self::Transcribe => \"transcribe finalized audio\",",
    "            Self::FinalizeRecording => \"finalize recording\",\n            Self::VoiceActivity => \"analyze finalized audio for speech\",\n            Self::Transcribe => \"transcribe finalized audio\",",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "    pub fn transcribe(\n        &self,\n        session_id: SessionId,",
    "    pub fn analyze_speech(\n        &self,\n        session_id: SessionId,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<VadAnalysis, RuntimeError> {\n        self.ensure_state(session_id, SessionState::Transcribing)?;\n        let mut recording = self.take_finalized(session_id, RuntimeOperation::VoiceActivity)?;\n        let result = recording.analyze_and_trim_speech(detector, config);\n        if !self.restore_finalized(session_id, recording) {\n            return Err(RuntimeError::WorkInvalidated {\n                session_id,\n                operation: RuntimeOperation::VoiceActivity,\n            });\n        }\n        result.map_err(RuntimeError::Pipeline)\n    }\n\n    pub fn transcribe(\n        &self,\n        session_id: SessionId,",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "        let recording = self.take_finalized_for_transcription(session_id)?;",
    "        let recording = self.take_finalized(session_id, RuntimeOperation::Transcribe)?;",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "    fn take_finalized_for_transcription(\n        &self,\n        session_id: SessionId,\n    ) -> Result<FinalizedRecording, RuntimeError> {",
    "    fn take_finalized(\n        &self,\n        session_id: SessionId,\n        operation: RuntimeOperation,\n    ) -> Result<FinalizedRecording, RuntimeError> {",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "                    operation: RuntimeOperation::Transcribe,\n                })\n            }\n        }\n    }\n\n    fn restore_finalized",
    "                    operation,\n                })\n            }\n        }\n    }\n\n    fn restore_finalized",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "        DictationPipelineError::InvalidAsrAudio(_) | DictationPipelineError::Recognition(_) => {\n            FailureStage::Internal\n        }",
    "        DictationPipelineError::VoiceActivity(_) => FailureStage::SpeechDetection,\n        DictationPipelineError::InvalidAsrAudio(_) | DictationPipelineError::Recognition(_) => {\n            FailureStage::Internal\n        }",
)

# Desktop capture bridge: expose VAD without giving UI/runtime ownership of finalized audio.
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    "use blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\n",
    "use blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VoiceActivityDetector};\n",
)
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    "    pub fn transcribe_dictation(\n        &self,",
    "    pub fn analyze_dictation_speech(\n        &self,\n        session_id: SessionId,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<VadAnalysis, DesktopCaptureError> {\n        self.runtime\n            .analyze_speech(session_id, detector, config)\n            .map_err(DesktopCaptureError::from)\n    }\n\n    pub fn transcribe_dictation(\n        &self,",
)
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    "    pub fn fail_dictation_recognition(\n        &self,",
    "    pub fn fail_dictation_speech_detection(\n        &self,\n        session_id: SessionId,\n    ) -> Result<SessionSnapshot, DesktopCaptureError> {\n        self.runtime\n            .fail(session_id, FailureStage::SpeechDetection)\n            .map_err(DesktopCaptureError::from)\n    }\n\n    pub fn fail_dictation_recognition(\n        &self,",
)

# Desktop service: production gets Silero; unit-test constructor remains detector-free unless explicitly injected.
replace_once(
    "apps/desktop/src-tauri/Cargo.toml",
    "blcvoice-shortcuts = { path = \"../../../crates/blcvoice-shortcuts\" }\n",
    "blcvoice-shortcuts = { path = \"../../../crates/blcvoice-shortcuts\" }\nblcvoice-vad = { path = \"../../../crates/blcvoice-vad\" }\nblcvoice-vad-silero = { path = \"../../../crates/blcvoice-vad-silero\" }\n",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\n",
    "use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\nuse blcvoice_vad::{VadAnalysis, VadConfig, VadError, VoiceActivityDetector};\nuse blcvoice_vad_silero::SileroVoiceActivityDetector;\n",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    Capture,\n    Transcription,\n    Insertion,",
    "    Capture,\n    NoSpeech,\n    SpeechDetection,\n    Transcription,\n    Insertion,",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    recognizers: Arc<dyn RecognizerFactory>,\n    recognizer_cache: Mutex<Option<CachedRecognizer>>,",
    "    recognizers: Arc<dyn RecognizerFactory>,\n    vad: Option<Mutex<Box<dyn VoiceActivityDetector>>>,\n    vad_config: VadConfig,\n    recognizer_cache: Mutex<Option<CachedRecognizer>>,",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {\n        Self::new(capture, Arc::new(TranscribeRecognizerFactory))\n    }\n\n    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {\n        Self {\n            capture,\n            recognizers,\n            recognizer_cache: Mutex::new(None),\n            slot: Mutex::new(DictationSlot::Idle),\n        }\n    }",
    "    pub fn production(capture: Arc<DesktopCaptureService>) -> Self {\n        Self::new_with_vad(\n            capture,\n            Arc::new(TranscribeRecognizerFactory),\n            Some(Box::new(SileroVoiceActivityDetector::new())),\n            VadConfig::default(),\n        )\n    }\n\n    fn new(capture: Arc<DesktopCaptureService>, recognizers: Arc<dyn RecognizerFactory>) -> Self {\n        Self::new_with_vad(capture, recognizers, None, VadConfig::default())\n    }\n\n    fn new_with_vad(\n        capture: Arc<DesktopCaptureService>,\n        recognizers: Arc<dyn RecognizerFactory>,\n        vad: Option<Box<dyn VoiceActivityDetector>>,\n        vad_config: VadConfig,\n    ) -> Self {\n        Self {\n            capture,\n            recognizers,\n            vad: vad.map(Mutex::new),\n            vad_config,\n            recognizer_cache: Mutex::new(None),\n            slot: Mutex::new(DictationSlot::Idle),\n        }\n    }",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "        let engine_id = active.recognizer.engine_id().to_owned();",
    "        if let Some(vad) = &self.vad {\n            let analysis = {\n                let mut detector = vad\n                    .lock()\n                    .unwrap_or_else(|poisoned| poisoned.into_inner());\n                self.capture.analyze_dictation_speech(\n                    session_id,\n                    detector.as_mut(),\n                    self.vad_config,\n                )\n            };\n            match analysis {\n                Ok(analysis) if !analysis.contains_speech() => {\n                    self.recycle_recognizer(active.recognizer_key, active.recognizer);\n                    let _ = self.capture.cancel_dictation(session_id);\n                    self.reset_to_idle();\n                    return Err(DesktopDictationError::new(\n                        DesktopDictationErrorKind::NoSpeech,\n                        \"no speech was detected; dictation was cancelled without running ASR\",\n                    ));\n                }\n                Ok(_) => {}\n                Err(error) => {\n                    self.recycle_recognizer(active.recognizer_key, active.recognizer);\n                    let _ = self.capture.fail_dictation_speech_detection(session_id);\n                    self.reset_to_idle();\n                    return Err(DesktopDictationError::new(\n                        DesktopDictationErrorKind::SpeechDetection,\n                        format!(\"dictation speech detection failed: {error}\"),\n                    ));\n                }\n            }\n        }\n\n        let engine_id = active.recognizer.engine_id().to_owned();",
)
# Tests: deterministic no-speech detector proves ASR is skipped through the production integration seam.
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    #[derive(Debug)]\n    struct FakeRecognizerFactory;",
    "    #[derive(Debug)]\n    struct AlwaysSilentDetector;\n\n    impl VoiceActivityDetector for AlwaysSilentDetector {\n        fn backend_name(&self) -> &'static str {\n            \"test-silence\"\n        }\n\n        fn analyze_mono(\n            &mut self,\n            samples: &[f32],\n            sample_rate_hz: u32,\n            _config: VadConfig,\n        ) -> Result<VadAnalysis, VadError> {\n            VadAnalysis::new(sample_rate_hz, samples.len(), Vec::new(), Some(0.0))\n        }\n    }\n\n    #[derive(Debug)]\n    struct FakeRecognizerFactory;",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    fn request() -> DesktopDictationRequest {",
    "    fn service_with_vad(\n        recognizers: Arc<dyn RecognizerFactory>,\n        detector: Box<dyn VoiceActivityDetector>,\n    ) -> DesktopDictationService {\n        let capture = Arc::new(DesktopCaptureService::new(\n            Arc::new(FakeDiscovery),\n            Arc::new(FakeCaptureFactory),\n        ));\n        DesktopDictationService::new_with_vad(\n            capture,\n            recognizers,\n            Some(detector),\n            VadConfig::default(),\n        )\n    }\n\n    fn request() -> DesktopDictationRequest {",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    #[test]\n    fn model_is_prepared_before_capture_starts() {",
    "    #[test]\n    fn no_speech_cancels_cleanly_before_asr() {\n        let service = service_with_vad(\n            Arc::new(FakeRecognizerFactory),\n            Box::new(AlwaysSilentDetector),\n        );\n        let session = service.start(request()).expect(\"dictation must start\");\n        let error = service.finish(session.id).expect_err(\"silence must skip ASR\");\n        assert_eq!(error.kind(), DesktopDictationErrorKind::NoSpeech);\n        assert_eq!(service.state_name(), \"idle\");\n        assert_eq!(\n            service.capture.current_session().map(|session| session.state),\n            Some(blcvoice_core::SessionState::Cancelled)\n        );\n    }\n\n    #[test]\n    fn model_is_prepared_before_capture_starts() {",
)

# IPC and presentation semantics: no-speech is a clean outcome, not a red failure state.
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "            DesktopDictationErrorKind::Capture => \"dictation_capture_failed\",\n            DesktopDictationErrorKind::Transcription => \"dictation_transcription_failed\",",
    "            DesktopDictationErrorKind::Capture => \"dictation_capture_failed\",\n            DesktopDictationErrorKind::NoSpeech => \"dictation_no_speech\",\n            DesktopDictationErrorKind::SpeechDetection => \"dictation_speech_detection_failed\",\n            DesktopDictationErrorKind::Transcription => \"dictation_transcription_failed\",",
)
replace_once(
    "apps/desktop/src-tauri/src/coordinator.rs",
    "    fn from_error(session_id: Option<SessionId>, error: &CommandErrorDto) -> Self {\n        Self::failure(\n            session_id,\n            error.code(),\n            error.message(),\n            error.recoverable_text().map(str::to_owned),\n        )\n    }",
    "    fn from_error(session_id: Option<SessionId>, error: &CommandErrorDto) -> Self {\n        if error.code() == \"dictation_no_speech\" {\n            return Self {\n                source: \"shortcut\",\n                state: \"noSpeech\",\n                session_id: session_id.map(SessionId::get),\n                text: None,\n                insertion_backend: None,\n                error_code: None,\n                message: Some(error.message().to_owned()),\n                recoverable_text: None,\n            };\n        }\n        Self::failure(\n            session_id,\n            error.code(),\n            error.message(),\n            error.recoverable_text().map(str::to_owned),\n        )\n    }",
)
replace_once(
    "apps/desktop/ui/app.js",
    "  } catch (error) {\n    state.dictationSessionId = null;\n    setPill(elements.dictationState, \"Failed\", \"failed\");\n    elements.dictationMessage.textContent = \"The dictation pipeline stopped before a clean completion.\";\n    showDictationError(error);\n  } finally {",
    "  } catch (error) {\n    state.dictationSessionId = null;\n    if (error && typeof error === \"object\" && error.code === \"dictation_no_speech\") {\n      clearDictationError();\n      setPill(elements.dictationState, \"Ready\", \"idle\");\n      elements.dictationMessage.textContent = \"No speech was detected. Nothing was transcribed or inserted.\";\n    } else {\n      setPill(elements.dictationState, \"Failed\", \"failed\");\n      elements.dictationMessage.textContent = \"The dictation pipeline stopped before a clean completion.\";\n      showDictationError(error);\n    }\n  } finally {",
)
replace_once(
    "apps/desktop/ui/app.js",
    "    case \"failed\":\n      state.shortcutSessionActive = false;",
    "    case \"noSpeech\":\n      state.shortcutSessionActive = false;\n      state.dictationBusy = false;\n      state.dictationSessionId = null;\n      clearDictationError();\n      setPill(elements.dictationState, \"Ready\", \"idle\");\n      elements.dictationMessage.textContent = \"No speech was detected. Nothing was transcribed or inserted.\";\n      break;\n    case \"failed\":\n      state.shortcutSessionActive = false;",
)
