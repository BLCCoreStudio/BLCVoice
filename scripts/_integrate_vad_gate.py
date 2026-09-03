from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    s = p.read_text()
    if old not in s:
        raise SystemExit(f"marker not found in {path}: {old[:120]!r}")
    p.write_text(s.replace(old, new, 1))

# audio-processing: safe frame-range retain primitive.
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "    InvalidUtteranceLimit,\n    UnsupportedChannelConversion {",
    "    InvalidUtteranceLimit,\n    InvalidFrameRange {\n        start_frame: usize,\n        end_frame: usize,\n        available_frames: usize,\n    },\n    UnsupportedChannelConversion {",
)
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "            Self::InvalidUtteranceLimit => {\n                formatter.write_str(\"utterance limit must allow at least one source frame\")\n            }\n            Self::UnsupportedChannelConversion { source, target } => write!(",
    "            Self::InvalidUtteranceLimit => {\n                formatter.write_str(\"utterance limit must allow at least one source frame\")\n            }\n            Self::InvalidFrameRange {\n                start_frame,\n                end_frame,\n                available_frames,\n            } => write!(\n                formatter,\n                \"invalid audio frame range {start_frame}..{end_frame} for {available_frames} available frames\"\n            ),\n            Self::UnsupportedChannelConversion { source, target } => write!(",
)
replace_once(
    "crates/blcvoice-audio-processing/src/lib.rs",
    "    /// Clear the utterance while retaining allocated capacity for the next dictation.\n    pub fn clear(&mut self) {\n        self.samples.clear();\n    }\n}",
    "    /// Retain one contiguous source-frame range without changing the native audio format.\n    ///\n    /// This is intended for worker-side leading/trailing silence trimming after speech detection.\n    /// Internal pauses remain untouched because only the outer envelope is retained.\n    pub fn retain_frame_range(\n        &mut self,\n        start_frame: usize,\n        end_frame: usize,\n    ) -> Result<(), ProcessingError> {\n        let available_frames = self.frames();\n        if start_frame >= end_frame || end_frame > available_frames {\n            return Err(ProcessingError::InvalidFrameRange {\n                start_frame,\n                end_frame,\n                available_frames,\n            });\n        }\n        let channels = usize::from(self.format.channels);\n        let start_sample = start_frame\n            .checked_mul(channels)\n            .ok_or(ProcessingError::BufferSizeOverflow)?;\n        let end_sample = end_frame\n            .checked_mul(channels)\n            .ok_or(ProcessingError::BufferSizeOverflow)?;\n        let retained_samples = end_sample - start_sample;\n        if start_sample != 0 {\n            self.samples.copy_within(start_sample..end_sample, 0);\n        }\n        self.samples.truncate(retained_samples);\n        Ok(())\n    }\n\n    /// Clear the utterance while retaining allocated capacity for the next dictation.\n    pub fn clear(&mut self) {\n        self.samples.clear();\n    }\n}",
)

# dictation crate dependency and speech preparation.
replace_once(
    "crates/blcvoice-dictation/Cargo.toml",
    "blcvoice-audio-processing = { path = \"../blcvoice-audio-processing\" }\n",
    "blcvoice-audio-processing = { path = \"../blcvoice-audio-processing\" }\nblcvoice-vad = { path = \"../blcvoice-vad\" }\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\n",
    "use blcvoice_audio_processing::{\n    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,\n};\nuse blcvoice_vad::{VadConfig, VadError, VoiceActivityDetector};\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "#[derive(Debug)]\npub struct FinalizedRecording {\n    utterance: UtteranceBuffer,\n    capture_stats: CaptureStats,\n}\n",
    "#[derive(Debug)]\npub struct FinalizedRecording {\n    utterance: UtteranceBuffer,\n    capture_stats: CaptureStats,\n}\n\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct SpeechPreparationReport {\n    pub contains_speech: bool,\n    pub original_source_frames: usize,\n    pub retained_source_frames: usize,\n    pub max_speech_probability: Option<f32>,\n}\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    #[must_use]\n    pub const fn capture_stats(&self) -> CaptureStats {\n        self.capture_stats\n    }\n\n    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.",
    "    #[must_use]\n    pub const fn capture_stats(&self) -> CaptureStats {\n        self.capture_stats\n    }\n\n    /// Detect speech on a mono view of the finalized native recording and retain only the outer\n    /// speech envelope. The detector runs at the source sample rate, so detector sample offsets map\n    /// exactly to native source-frame offsets. Internal silence is deliberately preserved.\n    pub fn prepare_speech(\n        &mut self,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<SpeechPreparationReport, DictationPipelineError> {\n        let source_format = self.utterance.format();\n        let analysis_format = ProcessingAudioFormat::new(1, source_format.sample_rate_hz())\n            .map_err(DictationPipelineError::Processing)?;\n        let original_source_frames = self.utterance.frames();\n        let analysis = {\n            let mut preprocessor = AudioPreprocessor::new(source_format, analysis_format)\n                .map_err(DictationPipelineError::Processing)?;\n            let processed = preprocessor\n                .process_utterance(self.utterance.as_interleaved())\n                .map_err(DictationPipelineError::Processing)?;\n            detector\n                .analyze_mono(processed.samples(), source_format.sample_rate_hz(), config)\n                .map_err(DictationPipelineError::VoiceActivity)?\n        };\n\n        let Some(envelope) = analysis.speech_envelope() else {\n            return Ok(SpeechPreparationReport {\n                contains_speech: false,\n                original_source_frames,\n                retained_source_frames: original_source_frames,\n                max_speech_probability: analysis.max_speech_probability,\n            });\n        };\n        self.utterance\n            .retain_frame_range(envelope.start_sample, envelope.end_sample)\n            .map_err(DictationPipelineError::Processing)?;\n        Ok(SpeechPreparationReport {\n            contains_speech: true,\n            original_source_frames,\n            retained_source_frames: self.utterance.frames(),\n            max_speech_probability: analysis.max_speech_probability,\n        })\n    }\n\n    /// Preprocess this finalized utterance to the recognizer's required format and run ASR.",
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
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "            Self::InvalidConfiguration(_)\n            | Self::InvalidCaptureRead { .. }",
    "            Self::InvalidConfiguration(_)\n            | Self::InvalidCaptureRead { .. }",
)
# No extra source-match change needed above: VoiceActivity has a source arm.

# Add deterministic fake-VAD tests inside dictation tests.
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    use blcvoice_audio::{AudioSampleFormat, AudioStreamConfig};\n",
    "    use blcvoice_audio::{AudioSampleFormat, AudioStreamConfig};\n    use blcvoice_vad::{SpeechRange, VadAnalysis};\n",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    struct FakeCapture {",
    "    struct FakeVad {\n        ranges: Vec<SpeechRange>,\n    }\n\n    impl VoiceActivityDetector for FakeVad {\n        fn backend_name(&self) -> &'static str {\n            \"fake-vad\"\n        }\n\n        fn analyze_mono(\n            &mut self,\n            samples: &[f32],\n            sample_rate_hz: u32,\n            _config: VadConfig,\n        ) -> Result<VadAnalysis, VadError> {\n            VadAnalysis::new(sample_rate_hz, samples.len(), self.ranges.clone(), Some(0.9))\n        }\n    }\n\n    struct FakeCapture {",
)
replace_once(
    "crates/blcvoice-dictation/src/lib.rs",
    "    #[test]\n    fn finalized_audio_downmixes_resamples_and_transcribes() {",
    "    #[test]\n    fn speech_preparation_trims_only_outer_silence() {\n        let source_frames = 4_800usize;\n        let capture = Box::new(FakeCapture::normal(\n            stereo_48k(),\n            stereo_samples(source_frames),\n        ));\n        let collector = RecordingCollector::with_read_frames(capture, 1_000, 128)\n            .expect(\"collector must initialize\");\n        let mut finalized = collector.finalize().expect(\"capture must finalize\");\n        let mut detector = FakeVad {\n            ranges: vec![\n                SpeechRange::new(800, 1_600).unwrap(),\n                SpeechRange::new(2_400, 4_000).unwrap(),\n            ],\n        };\n\n        let report = finalized\n            .prepare_speech(&mut detector, VadConfig::default())\n            .expect(\"speech analysis must succeed\");\n\n        assert!(report.contains_speech);\n        assert_eq!(report.original_source_frames, 4_800);\n        assert_eq!(report.retained_source_frames, 3_200);\n        assert_eq!(finalized.source_frames(), 3_200);\n        // The 800-frame internal pause remains because the outer envelope 800..4000 is retained.\n        assert_eq!(report.retained_source_frames, 4_000 - 800);\n    }\n\n    #[test]\n    fn no_speech_does_not_mutate_finalized_audio() {\n        let capture = Box::new(FakeCapture::normal(stereo_48k(), stereo_samples(4_800)));\n        let collector = RecordingCollector::new(capture, 1_000).expect(\"collector must initialize\");\n        let mut finalized = collector.finalize().expect(\"capture must finalize\");\n        let mut detector = FakeVad { ranges: Vec::new() };\n\n        let report = finalized\n            .prepare_speech(&mut detector, VadConfig::default())\n            .expect(\"silence analysis must succeed\");\n\n        assert!(!report.contains_speech);\n        assert_eq!(finalized.source_frames(), 4_800);\n        assert_eq!(report.retained_source_frames, 4_800);\n    }\n\n    #[test]\n    fn finalized_audio_downmixes_resamples_and_transcribes() {",
)

# runtime: expose speech preparation while finalized audio remains privately owned.
replace_once(
    "crates/blcvoice-runtime/Cargo.toml",
    "blcvoice-dictation = { path = \"../blcvoice-dictation\" }\n",
    "blcvoice-dictation = { path = \"../blcvoice-dictation\" }\nblcvoice-vad = { path = \"../blcvoice-vad\" }\n",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector,\n};\n",
    "use blcvoice_dictation::{\n    CaptureTranscription, DictationPipelineError, FinalizedRecording, PumpReport,\n    RecordingCollector, SpeechPreparationReport,\n};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\n",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "    FinalizeRecording,\n    Transcribe,",
    "    FinalizeRecording,\n    PrepareSpeech,\n    Transcribe,",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "            Self::FinalizeRecording => \"finalize recording\",\n            Self::Transcribe => \"transcribe finalized audio\",",
    "            Self::FinalizeRecording => \"finalize recording\",\n            Self::PrepareSpeech => \"analyze finalized audio for speech\",\n            Self::Transcribe => \"transcribe finalized audio\",",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "    pub fn transcribe(\n        &self,",
    "    pub fn prepare_speech(\n        &self,\n        session_id: SessionId,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<SpeechPreparationReport, RuntimeError> {\n        self.ensure_state(session_id, SessionState::Transcribing)?;\n        let result = {\n            let mut work = self.lock_work();\n            match &mut *work {\n                WorkSlot::Finalized {\n                    session_id: owned_id,\n                    recording,\n                } if *owned_id == session_id => recording.prepare_speech(detector, config),\n                _ => {\n                    return Err(RuntimeError::WorkInvalidated {\n                        session_id,\n                        operation: RuntimeOperation::PrepareSpeech,\n                    });\n                }\n            }\n        };\n        match result {\n            Ok(report) => Ok(report),\n            Err(error) => {\n                self.clear_work(session_id);\n                self.best_effort_fail(session_id, FailureStage::SpeechDetection);\n                Err(RuntimeError::Pipeline(error))\n            }\n        }\n    }\n\n    pub fn transcribe(\n        &self,",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "        DictationPipelineError::InvalidAsrAudio(_) | DictationPipelineError::Recognition(_) => {\n            FailureStage::Internal\n        }",
    "        DictationPipelineError::VoiceActivity(_) => FailureStage::SpeechDetection,\n        DictationPipelineError::InvalidAsrAudio(_) | DictationPipelineError::Recognition(_) => {\n            FailureStage::Internal\n        }",
)
replace_once(
    "crates/blcvoice-runtime/src/lib.rs",
    "        SessionState::Transcribing => RuntimeOperation::Transcribe,",
    "        SessionState::Transcribing => RuntimeOperation::Transcribe,",
)

# desktop capture bridge.
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    "use blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\n",
    "use blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError, RuntimeTranscription};\nuse blcvoice_vad::{VadConfig, VoiceActivityDetector};\nuse blcvoice_dictation::SpeechPreparationReport;\n",
)
replace_once(
    "apps/desktop/src-tauri/src/capture.rs",
    "    pub fn transcribe_dictation(\n        &self,",
    "    pub fn prepare_dictation_speech(\n        &self,\n        session_id: SessionId,\n        detector: &mut dyn VoiceActivityDetector,\n        config: VadConfig,\n    ) -> Result<SpeechPreparationReport, DesktopCaptureError> {\n        self.runtime\n            .prepare_speech(session_id, detector, config)\n            .map_err(DesktopCaptureError::from)\n    }\n\n    pub fn transcribe_dictation(\n        &self,",
)

# desktop dependencies.
replace_once(
    "apps/desktop/src-tauri/Cargo.toml",
    "blcvoice-shortcuts = { path = \"../../../crates/blcvoice-shortcuts\" }\n",
    "blcvoice-shortcuts = { path = \"../../../crates/blcvoice-shortcuts\" }\nblcvoice-vad = { path = \"../../../crates/blcvoice-vad\" }\nblcvoice-vad-silero = { path = \"../../../crates/blcvoice-vad-silero\" }\n",
)

# desktop dictation: no-speech gate and typed detection errors.
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\n",
    "use blcvoice_runtime::{FinalizationReport, RuntimeTranscription};\nuse blcvoice_vad::VadConfig;\nuse blcvoice_vad_silero::SileroVoiceActivityDetector;\n",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "    Capture,\n    Transcription,",
    "    Capture,\n    NoSpeech,\n    SpeechDetection,\n    Transcription,",
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    "        let engine_id = active.recognizer.engine_id().to_owned();",
    "        let mut detector = SileroVoiceActivityDetector::new();\n        let speech = match self.capture.prepare_dictation_speech(\n            session_id,\n            &mut detector,\n            VadConfig::default(),\n        ) {\n            Ok(report) => report,\n            Err(error) => {\n                self.recycle_recognizer(active.recognizer_key, active.recognizer);\n                self.reset_to_idle();\n                return Err(DesktopDictationError::new(\n                    DesktopDictationErrorKind::SpeechDetection,\n                    format!(\"dictation speech detection failed: {error}\"),\n                ));\n            }\n        };\n        if !speech.contains_speech {\n            self.recycle_recognizer(active.recognizer_key, active.recognizer);\n            let _ = self.capture.cancel_dictation(session_id);\n            self.reset_to_idle();\n            return Err(DesktopDictationError::new(\n                DesktopDictationErrorKind::NoSpeech,\n                \"no speech was detected in the dictation\",\n            ));\n        }\n\n        let engine_id = active.recognizer.engine_id().to_owned();",
)

# IPC typed codes.
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "            DesktopDictationErrorKind::Capture => \"dictation_capture_failed\",\n            DesktopDictationErrorKind::Transcription => \"dictation_transcription_failed\",",
    "            DesktopDictationErrorKind::Capture => \"dictation_capture_failed\",\n            DesktopDictationErrorKind::NoSpeech => \"no_speech\",\n            DesktopDictationErrorKind::SpeechDetection => \"speech_detection_failed\",\n            DesktopDictationErrorKind::Transcription => \"dictation_transcription_failed\",",
)
