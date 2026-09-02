#![forbid(unsafe_code)]

use std::path::Path;

use blcvoice_asr::{
    AudioFormat, AudioInput, FeaturePreference, RecognitionError, RecognitionErrorKind,
    RecognitionOptions, RecognitionTask, RecognizerCapabilities, SpeechRecognizer,
    TimestampGranularity, TranscriptSegment, TranscriptToken, TranscriptWord, Transcription,
};
use transcribe_cpp::{
    Backend as NativeBackend, Error as NativeError, Feature as NativeFeature, Itn as NativeItn,
    Model, ModelOptions, Pnc as NativePnc, RunOptions as NativeRunOptions, Session, SessionOptions,
    Task as NativeTask, TimestampKind as NativeTimestampKind, Transcript as NativeTranscript,
};

pub const ENGINE_ID: &str = "transcribe.cpp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranscribeBackend {
    #[default]
    Auto,
    Cpu,
    CpuAccelerated,
    Metal,
    Vulkan,
    Cuda,
    Rocm,
}

impl TranscribeBackend {
    fn native(self) -> NativeBackend {
        match self {
            Self::Auto => NativeBackend::Auto,
            Self::Cpu => NativeBackend::Cpu,
            Self::CpuAccelerated => NativeBackend::CpuAccel,
            Self::Metal => NativeBackend::Metal,
            Self::Vulkan => NativeBackend::Vulkan,
            Self::Cuda => NativeBackend::Cuda,
            Self::Rocm => NativeBackend::Rocm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TranscribeRecognizerConfig {
    pub backend: TranscribeBackend,
    /// Native worker thread count. Zero delegates to transcribe.cpp's default.
    pub n_threads: i32,
}

pub struct TranscribeRecognizer {
    session: Session,
    model_id: String,
    backend_name: String,
    capabilities: RecognizerCapabilities,
}

impl std::fmt::Debug for TranscribeRecognizer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TranscribeRecognizer")
            .field("model_id", &self.model_id)
            .field("backend_name", &self.backend_name)
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl TranscribeRecognizer {
    pub fn load(
        path: impl AsRef<Path>,
        config: TranscribeRecognizerConfig,
    ) -> Result<Self, RecognitionError> {
        if config.n_threads < 0 {
            return Err(RecognitionError::new(
                RecognitionErrorKind::InvalidRequest,
                "transcribe.cpp thread count cannot be negative",
            ));
        }

        let path = path.as_ref();
        let load_options = ModelOptions {
            backend: config.backend.native(),
            device: None,
        };
        let model = Model::load_with(path, &load_options).map_err(map_native_error)?;
        let native_capabilities = model.capabilities();
        let sample_rate_hz =
            u32::try_from(native_capabilities.native_sample_rate).map_err(|_| {
                RecognitionError::new(
                    RecognitionErrorKind::ModelLoad,
                    format!(
                        "transcribe.cpp model reported invalid native sample rate {}",
                        native_capabilities.native_sample_rate
                    ),
                )
            })?;
        if sample_rate_hz == 0 {
            return Err(RecognitionError::new(
                RecognitionErrorKind::ModelLoad,
                "transcribe.cpp model reported a zero native sample rate",
            ));
        }

        let required_audio_format = AudioFormat::new(1, sample_rate_hz).map_err(|error| {
            RecognitionError::new(RecognitionErrorKind::ModelLoad, error.to_string())
        })?;
        let capabilities = RecognizerCapabilities {
            required_audio_format,
            languages: native_capabilities.languages,
            translation_targets: native_capabilities.translate_target_languages,
            max_timestamp_granularity: map_timestamp_kind(native_capabilities.max_timestamp_kind),
            supports_language_detection: native_capabilities.supports_language_detect,
            supports_translation: native_capabilities.supports_translate,
            supports_streaming: native_capabilities.supports_streaming,
            supports_cancellation: model.supports(NativeFeature::Cancellation),
            supports_punctuation_control: model.supports(NativeFeature::Pnc),
            supports_inverse_text_normalization_control: model.supports(NativeFeature::Itn),
            max_audio_ms: positive_u64(native_capabilities.max_audio_ms),
        };

        let backend_name = model.backend();
        let model_id = model_identifier(&model, path);
        let session_options = SessionOptions {
            n_threads: config.n_threads,
            ..SessionOptions::default()
        };
        let session = model
            .session_with(&session_options)
            .map_err(map_native_error)?;

        Ok(Self {
            session,
            model_id,
            backend_name,
            capabilities,
        })
    }
}

impl SpeechRecognizer for TranscribeRecognizer {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }

    fn capabilities(&self) -> &RecognizerCapabilities {
        &self.capabilities
    }

    fn transcribe(
        &mut self,
        input: AudioInput<'_>,
        options: &RecognitionOptions,
    ) -> Result<Transcription, RecognitionError> {
        if input.is_empty() {
            return Err(RecognitionError::new(
                RecognitionErrorKind::InvalidAudio,
                "cannot transcribe an empty audio buffer",
            ));
        }
        if input.format() != self.capabilities.required_audio_format {
            return Err(RecognitionError::new(
                RecognitionErrorKind::InvalidAudio,
                format!(
                    "transcribe.cpp requires {} Hz {}-channel audio, received {} Hz {}-channel audio",
                    self.capabilities.required_audio_format.sample_rate_hz(),
                    self.capabilities.required_audio_format.channels(),
                    input.format().sample_rate_hz(),
                    input.format().channels()
                ),
            ));
        }

        let run_options = build_run_options(options)?;
        self.session
            .run(input.samples(), &run_options)
            .map(map_transcript)
            .map_err(map_native_error)
    }
}

fn build_run_options(options: &RecognitionOptions) -> Result<NativeRunOptions, RecognitionError> {
    if options
        .language_hint
        .as_deref()
        .is_some_and(|language| language.trim().is_empty())
    {
        return Err(RecognitionError::new(
            RecognitionErrorKind::InvalidRequest,
            "language hint cannot be empty",
        ));
    }

    let (task, target_language) = match &options.task {
        RecognitionTask::Transcribe => (NativeTask::Transcribe, None),
        RecognitionTask::Translate { target_language } => {
            if target_language.trim().is_empty() {
                return Err(RecognitionError::new(
                    RecognitionErrorKind::InvalidRequest,
                    "translation target language cannot be empty",
                ));
            }
            (NativeTask::Translate, Some(target_language.clone()))
        }
    };

    Ok(NativeRunOptions {
        task,
        timestamps: native_timestamp_kind(options.timestamps),
        pnc: native_pnc(options.punctuation),
        itn: native_itn(options.inverse_text_normalization),
        language: options.language_hint.clone(),
        target_language,
        ..NativeRunOptions::default()
    })
}

fn native_timestamp_kind(kind: TimestampGranularity) -> NativeTimestampKind {
    match kind {
        TimestampGranularity::None => NativeTimestampKind::None,
        TimestampGranularity::Auto => NativeTimestampKind::Auto,
        TimestampGranularity::Segment => NativeTimestampKind::Segment,
        TimestampGranularity::Word => NativeTimestampKind::Word,
        TimestampGranularity::Token => NativeTimestampKind::Token,
    }
}

fn map_timestamp_kind(kind: NativeTimestampKind) -> TimestampGranularity {
    match kind {
        NativeTimestampKind::None => TimestampGranularity::None,
        NativeTimestampKind::Auto => TimestampGranularity::Auto,
        NativeTimestampKind::Segment => TimestampGranularity::Segment,
        NativeTimestampKind::Word => TimestampGranularity::Word,
        NativeTimestampKind::Token => TimestampGranularity::Token,
    }
}

fn native_pnc(preference: FeaturePreference) -> NativePnc {
    match preference {
        FeaturePreference::ModelDefault => NativePnc::Default,
        FeaturePreference::Disabled => NativePnc::Off,
        FeaturePreference::Enabled => NativePnc::On,
    }
}

fn native_itn(preference: FeaturePreference) -> NativeItn {
    match preference {
        FeaturePreference::ModelDefault => NativeItn::Default,
        FeaturePreference::Disabled => NativeItn::Off,
        FeaturePreference::Enabled => NativeItn::On,
    }
}

fn map_transcript(transcript: NativeTranscript) -> Transcription {
    let raw_text = if transcript.raw_text.is_empty() {
        None
    } else {
        Some(transcript.raw_text)
    };

    Transcription {
        text: transcript.text,
        raw_text,
        detected_language: transcript.language,
        segments: transcript
            .segments
            .into_iter()
            .map(|segment| TranscriptSegment {
                start_ms: segment.t0_ms,
                end_ms: segment.t1_ms,
                text: segment.text,
                speaker_id: positive_u32(segment.speaker_id),
            })
            .collect(),
        words: transcript
            .words
            .into_iter()
            .map(|word| TranscriptWord {
                start_ms: word.t0_ms,
                end_ms: word.t1_ms,
                text: word.text,
            })
            .collect(),
        tokens: transcript
            .tokens
            .into_iter()
            .map(|token| TranscriptToken {
                start_ms: token.t0_ms,
                end_ms: token.t1_ms,
                text: token.text,
                confidence: token.p.is_finite().then_some(token.p),
            })
            .collect(),
    }
}

fn map_native_error(error: NativeError) -> RecognitionError {
    match error {
        NativeError::InvalidArgument(message) => {
            RecognitionError::new(RecognitionErrorKind::InvalidRequest, message)
        }
        NativeError::Nul(error) => {
            RecognitionError::new(RecognitionErrorKind::InvalidRequest, error.to_string())
        }
        NativeError::NotImplemented(message) | NativeError::Unsupported(message) => {
            RecognitionError::new(RecognitionErrorKind::Unsupported, message)
        }
        NativeError::ModelFileNotFound(message) => {
            RecognitionError::new(RecognitionErrorKind::ModelNotFound, message)
        }
        NativeError::ModelLoad(message) => {
            RecognitionError::new(RecognitionErrorKind::ModelLoad, message)
        }
        NativeError::OutOfMemory(message) => {
            RecognitionError::new(RecognitionErrorKind::ResourceExhausted, message)
        }
        NativeError::Backend(message) => {
            RecognitionError::new(RecognitionErrorKind::BackendUnavailable, message)
        }
        NativeError::InputTooLong(message) => {
            RecognitionError::new(RecognitionErrorKind::InputTooLong, message)
        }
        NativeError::Aborted { message, partial } => recognition_error_with_optional_partial(
            RecognitionErrorKind::Cancelled,
            message,
            partial.map(|transcript| map_transcript(*transcript)),
        ),
        NativeError::OutputTruncated { message, partial } => {
            recognition_error_with_optional_partial(
                RecognitionErrorKind::OutputTruncated,
                message,
                partial.map(|transcript| map_transcript(*transcript)),
            )
        }
        NativeError::Busy(message) => RecognitionError::new(RecognitionErrorKind::Busy, message),
        NativeError::BadStructSize(message) | NativeError::VersionMismatch(message) => {
            RecognitionError::new(RecognitionErrorKind::Internal, message)
        }
        other => RecognitionError::new(RecognitionErrorKind::Internal, other.to_string()),
    }
}

fn recognition_error_with_optional_partial(
    kind: RecognitionErrorKind,
    message: String,
    partial: Option<Transcription>,
) -> RecognitionError {
    match partial {
        Some(transcript) => RecognitionError::with_partial(kind, message, transcript),
        None => RecognitionError::new(kind, message),
    }
}

fn model_identifier(model: &Model, path: &Path) -> String {
    let arch = model.arch();
    let variant = model.variant();
    match (arch.is_empty(), variant.is_empty()) {
        (false, false) => format!("{arch}:{variant}"),
        (false, true) => arch,
        (true, false) => variant,
        (true, true) => path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
    }
}

fn positive_u32(value: i32) -> Option<u32> {
    u32::try_from(value).ok().filter(|value| *value > 0)
}

fn positive_u64(value: i64) -> Option<u64> {
    u64::try_from(value).ok().filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_mapping_is_explicit() {
        assert_eq!(TranscribeBackend::Cpu.native(), NativeBackend::Cpu);
        assert_eq!(
            TranscribeBackend::CpuAccelerated.native(),
            NativeBackend::CpuAccel
        );
        assert_eq!(TranscribeBackend::Vulkan.native(), NativeBackend::Vulkan);
    }

    #[test]
    fn default_dictation_options_disable_unrequested_timestamps() {
        let mapped = build_run_options(&RecognitionOptions::default()).expect("valid options");
        assert_eq!(mapped.task, NativeTask::Transcribe);
        assert_eq!(mapped.timestamps, NativeTimestampKind::None);
        assert_eq!(mapped.pnc, NativePnc::Default);
        assert_eq!(mapped.itn, NativeItn::Default);
        assert_eq!(mapped.target_language, None);
    }

    #[test]
    fn translation_options_preserve_language_contract() {
        let options = RecognitionOptions {
            task: RecognitionTask::Translate {
                target_language: "en".to_owned(),
            },
            language_hint: Some("de".to_owned()),
            timestamps: TimestampGranularity::Word,
            punctuation: FeaturePreference::Enabled,
            inverse_text_normalization: FeaturePreference::Disabled,
        };
        let mapped = build_run_options(&options).expect("valid options");

        assert_eq!(mapped.task, NativeTask::Translate);
        assert_eq!(mapped.language.as_deref(), Some("de"));
        assert_eq!(mapped.target_language.as_deref(), Some("en"));
        assert_eq!(mapped.timestamps, NativeTimestampKind::Word);
        assert_eq!(mapped.pnc, NativePnc::On);
        assert_eq!(mapped.itn, NativeItn::Off);
    }

    #[test]
    fn blank_language_requests_are_rejected_before_ffi() {
        let options = RecognitionOptions {
            language_hint: Some("  ".to_owned()),
            ..RecognitionOptions::default()
        };
        assert_eq!(
            build_run_options(&options)
                .expect_err("blank language must fail")
                .kind(),
            RecognitionErrorKind::InvalidRequest
        );
    }

    #[test]
    fn native_transcript_is_materialized_without_engine_specific_handles() {
        let native = NativeTranscript {
            text: "hello".to_owned(),
            raw_text: " raw hello ".to_owned(),
            language: Some("en".to_owned()),
            ..NativeTranscript::default()
        };
        let mapped = map_transcript(native);
        assert_eq!(mapped.text, "hello");
        assert_eq!(mapped.raw_text.as_deref(), Some(" raw hello "));
        assert_eq!(mapped.detected_language.as_deref(), Some("en"));
    }

    #[test]
    fn native_resource_failures_remain_typed() {
        let error = map_native_error(NativeError::OutOfMemory("allocation failed".to_owned()));
        assert_eq!(error.kind(), RecognitionErrorKind::ResourceExhausted);
    }

    #[test]
    fn optional_limits_and_speaker_ids_ignore_non_positive_sentinels() {
        assert_eq!(positive_u64(0), None);
        assert_eq!(positive_u64(30_000), Some(30_000));
        assert_eq!(positive_u32(0), None);
        assert_eq!(positive_u32(2), Some(2));
    }
}
