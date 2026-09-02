#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// Signal shape expected by a speech-recognition engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFormat {
    channels: u16,
    sample_rate_hz: u32,
}

impl AudioFormat {
    pub fn new(channels: u16, sample_rate_hz: u32) -> Result<Self, AudioFormatError> {
        if channels == 0 || sample_rate_hz == 0 {
            return Err(AudioFormatError);
        }
        Ok(Self {
            channels,
            sample_rate_hz,
        })
    }

    #[must_use]
    pub fn channels(self) -> u16 {
        self.channels
    }

    #[must_use]
    pub fn sample_rate_hz(self) -> u32 {
        self.sample_rate_hz
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormatError;

impl fmt::Display for AudioFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ASR audio format must have non-zero channels and sample rate")
    }
}

impl Error for AudioFormatError {}

/// Borrowed normalized `f32` PCM presented to an ASR adapter.
///
/// Samples are interleaved, finite and constrained to `[-1.0, 1.0]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioInput<'a> {
    samples: &'a [f32],
    format: AudioFormat,
}

impl<'a> AudioInput<'a> {
    pub fn new(samples: &'a [f32], format: AudioFormat) -> Result<Self, AudioInputError> {
        let channels = usize::from(format.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(AudioInputError::UnalignedSamples {
                channels: format.channels,
                samples: samples.len(),
            });
        }

        for (index, sample) in samples.iter().copied().enumerate() {
            if !sample.is_finite() {
                return Err(AudioInputError::NonFiniteSample { index });
            }
            if !(-1.0..=1.0).contains(&sample) {
                return Err(AudioInputError::OutOfRangeSample { index });
            }
        }

        Ok(Self { samples, format })
    }

    #[must_use]
    pub fn samples(self) -> &'a [f32] {
        self.samples
    }

    #[must_use]
    pub fn format(self) -> AudioFormat {
        self.format
    }

    #[must_use]
    pub fn frames(self) -> usize {
        self.samples.len() / usize::from(self.format.channels)
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.samples.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioInputError {
    UnalignedSamples { channels: u16, samples: usize },
    NonFiniteSample { index: usize },
    OutOfRangeSample { index: usize },
}

impl fmt::Display for AudioInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnalignedSamples { channels, samples } => write!(
                formatter,
                "{samples} samples do not form complete {channels}-channel ASR frames"
            ),
            Self::NonFiniteSample { index } => {
                write!(formatter, "ASR sample at index {index} is not finite")
            }
            Self::OutOfRangeSample { index } => write!(
                formatter,
                "ASR sample at index {index} is outside the normalized [-1, 1] range"
            ),
        }
    }
}

impl Error for AudioInputError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimestampGranularity {
    #[default]
    None,
    Auto,
    Segment,
    Word,
    Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FeaturePreference {
    #[default]
    ModelDefault,
    Disabled,
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RecognitionTask {
    #[default]
    Transcribe,
    Translate {
        target_language: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognitionOptions {
    pub task: RecognitionTask,
    pub language_hint: Option<String>,
    pub timestamps: TimestampGranularity,
    pub punctuation: FeaturePreference,
    pub inverse_text_normalization: FeaturePreference,
}

impl Default for RecognitionOptions {
    fn default() -> Self {
        Self {
            task: RecognitionTask::Transcribe,
            language_hint: None,
            timestamps: TimestampGranularity::None,
            punctuation: FeaturePreference::ModelDefault,
            inverse_text_normalization: FeaturePreference::ModelDefault,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizerCapabilities {
    pub required_audio_format: AudioFormat,
    pub languages: Vec<String>,
    pub translation_targets: Vec<String>,
    pub max_timestamp_granularity: TimestampGranularity,
    pub supports_language_detection: bool,
    pub supports_translation: bool,
    pub supports_streaming: bool,
    pub supports_cancellation: bool,
    pub supports_punctuation_control: bool,
    pub supports_inverse_text_normalization_control: bool,
    pub max_audio_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub speaker_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptWord {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptToken {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcription {
    pub text: String,
    pub raw_text: Option<String>,
    pub detected_language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
    pub words: Vec<TranscriptWord>,
    pub tokens: Vec<TranscriptToken>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognitionErrorKind {
    InvalidAudio,
    InvalidRequest,
    ModelNotFound,
    ModelLoad,
    BackendUnavailable,
    Unsupported,
    ResourceExhausted,
    InputTooLong,
    Cancelled,
    Busy,
    OutputTruncated,
    Internal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionError {
    kind: RecognitionErrorKind,
    message: String,
    partial: Option<Box<Transcription>>,
}

impl RecognitionError {
    #[must_use]
    pub fn new(kind: RecognitionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            partial: None,
        }
    }

    #[must_use]
    pub fn with_partial(
        kind: RecognitionErrorKind,
        message: impl Into<String>,
        partial: Transcription,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            partial: Some(Box::new(partial)),
        }
    }

    #[must_use]
    pub fn kind(&self) -> RecognitionErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn partial(&self) -> Option<&Transcription> {
        self.partial.as_deref()
    }
}

impl fmt::Display for RecognitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for RecognitionError {}

/// Stable engine boundary consumed by the BLCVoice application layer.
pub trait SpeechRecognizer: Send {
    fn engine_id(&self) -> &'static str;
    fn model_id(&self) -> &str;
    fn backend_name(&self) -> &str;
    fn capabilities(&self) -> &RecognizerCapabilities;

    fn transcribe(
        &mut self,
        input: AudioInput<'_>,
        options: &RecognitionOptions,
    ) -> Result<Transcription, RecognitionError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mono_16k() -> AudioFormat {
        AudioFormat::new(1, 16_000).expect("valid test format")
    }

    #[test]
    fn rejects_invalid_audio_formats() {
        assert_eq!(AudioFormat::new(0, 16_000), Err(AudioFormatError));
        assert_eq!(AudioFormat::new(1, 0), Err(AudioFormatError));
    }

    #[test]
    fn audio_input_requires_complete_interleaved_frames() {
        let stereo = AudioFormat::new(2, 48_000).expect("valid test format");
        assert!(matches!(
            AudioInput::new(&[0.1, 0.2, 0.3], stereo),
            Err(AudioInputError::UnalignedSamples { .. })
        ));
    }

    #[test]
    fn audio_input_rejects_non_finite_and_out_of_range_samples() {
        assert!(matches!(
            AudioInput::new(&[f32::NAN], mono_16k()),
            Err(AudioInputError::NonFiniteSample { index: 0 })
        ));
        assert!(matches!(
            AudioInput::new(&[1.01], mono_16k()),
            Err(AudioInputError::OutOfRangeSample { index: 0 })
        ));
    }

    #[test]
    fn audio_input_reports_frames_without_copying() {
        let samples = [0.0, 0.25, -0.25];
        let input = AudioInput::new(&samples, mono_16k()).expect("valid audio input");
        assert_eq!(input.frames(), 3);
        assert_eq!(input.samples().as_ptr(), samples.as_ptr());
    }

    #[test]
    fn dictation_defaults_avoid_unrequested_alignment_work() {
        let options = RecognitionOptions::default();
        assert_eq!(options.task, RecognitionTask::Transcribe);
        assert_eq!(options.timestamps, TimestampGranularity::None);
        assert_eq!(options.punctuation, FeaturePreference::ModelDefault);
    }

    #[test]
    fn recognition_error_preserves_partial_transcript() {
        let partial = Transcription {
            text: "partial text".to_owned(),
            ..Transcription::default()
        };
        let error = RecognitionError::with_partial(
            RecognitionErrorKind::OutputTruncated,
            "decode budget exhausted",
            partial.clone(),
        );

        assert_eq!(error.kind(), RecognitionErrorKind::OutputTruncated);
        assert_eq!(error.partial(), Some(&partial));
    }
}
