#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

use blcvoice_asr::{
    AudioFormat as AsrAudioFormat, AudioInput as AsrAudioInput, AudioInputError, RecognitionError,
    RecognitionOptions, SpeechRecognizer, Transcription,
};
use blcvoice_audio::{AudioFailure, CaptureStats, InputCaptureSession};
use blcvoice_audio_processing::{
    AudioFormat as ProcessingAudioFormat, AudioPreprocessor, ProcessingError, UtteranceBuffer,
};

pub const DEFAULT_READ_FRAMES: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PumpReport {
    pub samples_read: usize,
    pub frames_read: usize,
    pub buffered_frames: usize,
    pub stats: CaptureStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureTranscription {
    pub transcription: Transcription,
    pub capture_stats: CaptureStats,
    pub source_frames: usize,
    pub asr_frames: usize,
}

#[derive(Debug)]
pub enum DictationPipelineError {
    InvalidConfiguration(&'static str),
    Capture(AudioFailure),
    InvalidCaptureRead {
        returned_samples: usize,
        scratch_capacity: usize,
        channels: u16,
    },
    CaptureIntegrity {
        stats: CaptureStats,
    },
    EmptyUtterance,
    Processing(ProcessingError),
    InvalidAsrAudio(AudioInputError),
    Recognition(RecognitionError),
}

impl fmt::Display for DictationPipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => formatter.write_str(message),
            Self::Capture(error) => write!(formatter, "audio capture failed: {error}"),
            Self::InvalidCaptureRead {
                returned_samples,
                scratch_capacity,
                channels,
            } => write!(
                formatter,
                "capture returned {returned_samples} samples into a {scratch_capacity}-sample scratch buffer for {channels}-channel audio"
            ),
            Self::CaptureIntegrity { stats } => write!(
                formatter,
                "capture integrity was lost: {} dropped samples, {} callback errors",
                stats.dropped_samples, stats.callback_errors
            ),
            Self::EmptyUtterance => {
                formatter.write_str("captured utterance contains no audio frames")
            }
            Self::Processing(error) => write!(formatter, "audio preprocessing failed: {error}"),
            Self::InvalidAsrAudio(error) => {
                write!(formatter, "processed ASR audio is invalid: {error}")
            }
            Self::Recognition(error) => write!(formatter, "speech recognition failed: {error}"),
        }
    }
}

impl Error for DictationPipelineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capture(error) => Some(error),
            Self::Processing(error) => Some(error),
            Self::InvalidAsrAudio(error) => Some(error),
            Self::Recognition(error) => Some(error),
            Self::InvalidConfiguration(_)
            | Self::InvalidCaptureRead { .. }
            | Self::CaptureIntegrity { .. }
            | Self::EmptyUtterance => None,
        }
    }
}

/// Worker-side bridge from an active device-native capture stream to one bounded utterance.
///
/// This type must be driven outside the realtime audio callback. `pump()` should be called
/// regularly while recording so the short ring buffer remains a realtime handoff rather than
/// accidentally becoming utterance storage.
pub struct RecordingCollector {
    capture: Box<dyn InputCaptureSession>,
    source_format: ProcessingAudioFormat,
    utterance: UtteranceBuffer,
    scratch: Vec<f32>,
}

impl fmt::Debug for RecordingCollector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordingCollector")
            .field("source_format", &self.source_format)
            .field("buffered_frames", &self.utterance.frames())
            .field("max_frames", &self.utterance.max_frames())
            .field("scratch_samples", &self.scratch.len())
            .field("capture_stats", &self.capture.stats())
            .finish_non_exhaustive()
    }
}

impl RecordingCollector {
    pub fn new(
        capture: Box<dyn InputCaptureSession>,
        max_duration_ms: u32,
    ) -> Result<Self, DictationPipelineError> {
        Self::with_read_frames(capture, max_duration_ms, DEFAULT_READ_FRAMES)
    }

    pub fn with_read_frames(
        capture: Box<dyn InputCaptureSession>,
        max_duration_ms: u32,
        read_frames: usize,
    ) -> Result<Self, DictationPipelineError> {
        if read_frames == 0 {
            return Err(DictationPipelineError::InvalidConfiguration(
                "capture worker read size must contain at least one frame",
            ));
        }

        let source_format = ProcessingAudioFormat::from_stream_config(capture.stream_config())
            .map_err(DictationPipelineError::Processing)?;
        let channels = usize::from(source_format.channels());
        let scratch_samples = read_frames.checked_mul(channels).ok_or(
            DictationPipelineError::InvalidConfiguration(
                "capture worker scratch buffer size overflowed",
            ),
        )?;
        let mut scratch = Vec::new();
        scratch.try_reserve_exact(scratch_samples).map_err(|_| {
            DictationPipelineError::InvalidConfiguration(
                "capture worker could not reserve its scratch buffer",
            )
        })?;
        scratch.resize(scratch_samples, 0.0);

        let utterance = UtteranceBuffer::with_max_duration_ms(source_format, max_duration_ms)
            .map_err(DictationPipelineError::Processing)?;

        Ok(Self {
            capture,
            source_format,
            utterance,
            scratch,
        })
    }

    #[must_use]
    pub fn source_format(&self) -> ProcessingAudioFormat {
        self.source_format
    }

    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        self.utterance.frames()
    }

    #[must_use]
    pub fn capture_stats(&self) -> CaptureStats {
        self.capture.stats()
    }

    /// Drain all samples currently available from the capture handoff.
    pub fn pump(&mut self) -> Result<PumpReport, DictationPipelineError> {
        let channels = usize::from(self.source_format.channels());
        let mut samples_read = 0usize;

        loop {
            let returned = self.capture.read_interleaved_f32(&mut self.scratch);
            if returned == 0 {
                break;
            }
            if returned > self.scratch.len() || !returned.is_multiple_of(channels) {
                return Err(DictationPipelineError::InvalidCaptureRead {
                    returned_samples: returned,
                    scratch_capacity: self.scratch.len(),
                    channels: self.source_format.channels(),
                });
            }

            self.utterance
                .push_interleaved(&self.scratch[..returned])
                .map_err(DictationPipelineError::Processing)?;
            samples_read = samples_read.checked_add(returned).ok_or(
                DictationPipelineError::InvalidConfiguration(
                    "capture worker sample counter overflowed",
                ),
            )?;
        }

        Ok(PumpReport {
            samples_read,
            frames_read: samples_read / channels,
            buffered_frames: self.utterance.frames(),
            stats: self.capture.stats(),
        })
    }

    /// Quiesce capture, drain its residual handoff, preprocess the complete utterance and run ASR.
    ///
    /// Any observed ring-buffer loss or callback failure invalidates the utterance. BLCVoice must
    /// never turn known-corrupted capture into an apparently successful dictation.
    pub fn finish(
        mut self,
        recognizer: &mut dyn SpeechRecognizer,
        options: &RecognitionOptions,
    ) -> Result<CaptureTranscription, DictationPipelineError> {
        self.capture
            .pause()
            .map_err(DictationPipelineError::Capture)?;
        self.pump()?;

        let capture_stats = self.capture.stats();
        if capture_stats.dropped_samples > 0 || capture_stats.callback_errors > 0 {
            return Err(DictationPipelineError::CaptureIntegrity {
                stats: capture_stats,
            });
        }
        if self.utterance.is_empty() {
            return Err(DictationPipelineError::EmptyUtterance);
        }

        let asr_format = recognizer.capabilities().required_audio_format;
        let processing_target = processing_format(asr_format)?;
        let mut preprocessor = AudioPreprocessor::new(self.source_format, processing_target)
            .map_err(DictationPipelineError::Processing)?;
        let source_frames = self.utterance.frames();
        let processed = preprocessor
            .process_utterance(self.utterance.as_interleaved())
            .map_err(DictationPipelineError::Processing)?;
        let asr_frames = processed.frames();
        let input = AsrAudioInput::new(processed.samples(), asr_format)
            .map_err(DictationPipelineError::InvalidAsrAudio)?;
        let transcription = recognizer
            .transcribe(input, options)
            .map_err(DictationPipelineError::Recognition)?;

        Ok(CaptureTranscription {
            transcription,
            capture_stats,
            source_frames,
            asr_frames,
        })
    }
}

fn processing_format(
    format: AsrAudioFormat,
) -> Result<ProcessingAudioFormat, DictationPipelineError> {
    ProcessingAudioFormat::new(format.channels(), format.sample_rate_hz())
        .map_err(DictationPipelineError::Processing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blcvoice_asr::{RecognitionError, RecognizerCapabilities, TimestampGranularity};
    use blcvoice_audio::{AudioSampleFormat, AudioStreamConfig};

    struct FakeCapture {
        config: AudioStreamConfig,
        samples: Vec<f32>,
        position: usize,
        stats: CaptureStats,
        forced_read: Option<usize>,
    }

    impl FakeCapture {
        fn normal(config: AudioStreamConfig, samples: Vec<f32>) -> Self {
            Self {
                config,
                samples,
                position: 0,
                stats: CaptureStats::default(),
                forced_read: None,
            }
        }
    }

    impl InputCaptureSession for FakeCapture {
        fn stream_config(&self) -> &AudioStreamConfig {
            &self.config
        }

        fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
            if let Some(returned) = self.forced_read.take() {
                let writable = returned.min(output.len()).min(self.samples.len());
                output[..writable].copy_from_slice(&self.samples[..writable]);
                return returned;
            }

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
            self.stats
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
        calls: usize,
        seen_frames: usize,
        seen_format: Option<AsrAudioFormat>,
    }

    impl FakeRecognizer {
        fn mono_16k() -> Self {
            Self {
                capabilities: RecognizerCapabilities {
                    required_audio_format: AsrAudioFormat::new(1, 16_000)
                        .expect("valid test ASR format"),
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
                calls: 0,
                seen_frames: 0,
                seen_format: None,
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
            input: AsrAudioInput<'_>,
            _options: &RecognitionOptions,
        ) -> Result<Transcription, RecognitionError> {
            self.calls += 1;
            self.seen_frames = input.frames();
            self.seen_format = Some(input.format());
            Ok(Transcription {
                text: "hello".to_owned(),
                ..Transcription::default()
            })
        }
    }

    fn stereo_48k() -> AudioStreamConfig {
        AudioStreamConfig {
            channels: 2,
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::F32,
        }
    }

    #[test]
    fn drains_capture_downmixes_resamples_and_transcribes() {
        let source_frames = 4_800usize;
        let mut samples = Vec::with_capacity(source_frames * 2);
        for _ in 0..source_frames {
            samples.extend_from_slice(&[0.2, 0.2]);
        }
        let capture = Box::new(FakeCapture::normal(stereo_48k(), samples));
        let collector = RecordingCollector::with_read_frames(capture, 1_000, 128)
            .expect("collector must initialize");
        let mut recognizer = FakeRecognizer::mono_16k();

        let result = collector
            .finish(&mut recognizer, &RecognitionOptions::default())
            .expect("capture-to-ASR pipeline must succeed");

        assert_eq!(result.transcription.text, "hello");
        assert_eq!(result.source_frames, source_frames);
        assert_eq!(result.asr_frames, 1_600);
        assert_eq!(recognizer.calls, 1);
        assert_eq!(recognizer.seen_frames, 1_600);
        assert_eq!(
            recognizer.seen_format,
            Some(AsrAudioFormat::new(1, 16_000).expect("valid test format"))
        );
    }

    #[test]
    fn capture_integrity_loss_blocks_recognition() {
        let mut capture = FakeCapture::normal(stereo_48k(), vec![0.1, 0.1, 0.2, 0.2]);
        capture.stats = CaptureStats {
            received_samples: 4,
            dropped_samples: 2,
            callback_errors: 1,
            last_failure: None,
        };
        let collector =
            RecordingCollector::new(Box::new(capture), 1_000).expect("collector must initialize");
        let mut recognizer = FakeRecognizer::mono_16k();

        let error = collector
            .finish(&mut recognizer, &RecognitionOptions::default())
            .expect_err("known capture loss must invalidate dictation");

        assert!(matches!(
            error,
            DictationPipelineError::CaptureIntegrity { .. }
        ));
        assert_eq!(recognizer.calls, 0);
    }

    #[test]
    fn empty_finalized_utterance_fails_before_asr() {
        let capture = Box::new(FakeCapture::normal(stereo_48k(), Vec::new()));
        let collector = RecordingCollector::new(capture, 1_000).expect("collector must initialize");
        let mut recognizer = FakeRecognizer::mono_16k();

        let error = collector
            .finish(&mut recognizer, &RecognitionOptions::default())
            .expect_err("empty capture must fail");

        assert!(matches!(error, DictationPipelineError::EmptyUtterance));
        assert_eq!(recognizer.calls, 0);
    }

    #[test]
    fn utterance_duration_limit_is_enforced() {
        let frames = 97usize;
        let samples = vec![0.1; frames * 2];
        let capture = Box::new(FakeCapture::normal(stereo_48k(), samples));
        let collector = RecordingCollector::with_read_frames(capture, 2, 128)
            .expect("collector must initialize");
        let mut recognizer = FakeRecognizer::mono_16k();

        let error = collector
            .finish(&mut recognizer, &RecognitionOptions::default())
            .expect_err("97 source frames exceed a 2 ms 48 kHz limit of 96 frames");

        assert!(matches!(
            error,
            DictationPipelineError::Processing(ProcessingError::UtteranceTooLong { .. })
        ));
        assert_eq!(recognizer.calls, 0);
    }

    #[test]
    fn defensive_alignment_check_rejects_broken_capture_adapter() {
        let mut capture = FakeCapture::normal(stereo_48k(), vec![0.1, 0.1, 0.2]);
        capture.forced_read = Some(3);
        let mut collector = RecordingCollector::with_read_frames(Box::new(capture), 1_000, 4)
            .expect("collector must initialize");

        let error = collector
            .pump()
            .expect_err("stereo capture cannot return three samples");

        assert!(matches!(
            error,
            DictationPipelineError::InvalidCaptureRead {
                returned_samples: 3,
                channels: 2,
                ..
            }
        ));
    }

    #[test]
    fn pump_reports_drained_and_buffered_frame_counts() {
        let capture = Box::new(FakeCapture::normal(
            stereo_48k(),
            vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.4, 0.4],
        ));
        let mut collector = RecordingCollector::with_read_frames(capture, 1_000, 2)
            .expect("collector must initialize");

        let report = collector.pump().expect("pump must succeed");

        assert_eq!(report.samples_read, 8);
        assert_eq!(report.frames_read, 4);
        assert_eq!(report.buffered_frames, 4);
        assert_eq!(collector.buffered_frames(), 4);
    }

    #[test]
    fn zero_read_frame_configuration_is_rejected() {
        let capture = Box::new(FakeCapture::normal(stereo_48k(), Vec::new()));
        assert!(matches!(
            RecordingCollector::with_read_frames(capture, 1_000, 0),
            Err(DictationPipelineError::InvalidConfiguration(_))
        ));
    }
}
