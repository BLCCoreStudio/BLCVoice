#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

use audioadapter_buffers::direct::InterleavedSlice;
use blcvoice_audio::AudioStreamConfig;
use rubato::{Fft, FixedSync, Resampler};

pub const DEFAULT_CHUNK_FRAMES: usize = 1_024;

/// Channel/rate shape of an interleaved `f32` PCM signal.
///
/// Capture keeps the device-native shape. An ASR adapter can request a different
/// target shape without forcing that target into the capture backend itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFormat {
    channels: u16,
    sample_rate_hz: u32,
}

impl AudioFormat {
    pub fn new(channels: u16, sample_rate_hz: u32) -> Result<Self, ProcessingError> {
        if channels == 0 || sample_rate_hz == 0 {
            return Err(ProcessingError::InvalidFormat);
        }

        Ok(Self {
            channels,
            sample_rate_hz,
        })
    }

    pub fn from_stream_config(config: &AudioStreamConfig) -> Result<Self, ProcessingError> {
        Self::new(config.channels, config.sample_rate_hz)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingError {
    InvalidFormat,
    InvalidChunkSize,
    UnsupportedChannelConversion { source: u16, target: u16 },
    InvalidInputLength { expected: usize, actual: usize },
    OutputTooSmall { required: usize, actual: usize },
    Adapter(String),
    ResamplerConstruction(String),
    Resampling(String),
}

impl fmt::Display for ProcessingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => {
                formatter.write_str("audio format must have non-zero channels and sample rate")
            }
            Self::InvalidChunkSize => {
                formatter.write_str("audio preprocessing chunk size must be non-zero")
            }
            Self::UnsupportedChannelConversion { source, target } => write!(
                formatter,
                "unsupported channel conversion from {source} channels to {target} channels"
            ),
            Self::InvalidInputLength { expected, actual } => write!(
                formatter,
                "audio preprocessing expected {expected} input samples but received {actual}"
            ),
            Self::OutputTooSmall { required, actual } => write!(
                formatter,
                "audio preprocessing requires at least {required} output samples but received {actual}"
            ),
            Self::Adapter(message) => write!(formatter, "audio buffer adapter failed: {message}"),
            Self::ResamplerConstruction(message) => {
                write!(formatter, "audio resampler construction failed: {message}")
            }
            Self::Resampling(message) => write!(formatter, "audio resampling failed: {message}"),
        }
    }
}

impl Error for ProcessingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessReport {
    pub input_frames: usize,
    pub output_frames: usize,
}

/// Reusable worker-side preprocessing stage between native capture and an ASR engine.
///
/// The stage never runs in the CPAL callback. Construction owns all reusable scratch
/// storage and the resampler instance; `process_block` performs no deliberate heap
/// allocation in BLCVoice code.
pub struct AudioPreprocessor {
    source: AudioFormat,
    target: AudioFormat,
    input_frames_per_block: usize,
    normalized_input: Vec<f32>,
    resampler: Option<Fft<f32>>,
    max_output_frames: usize,
}

impl fmt::Debug for AudioPreprocessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioPreprocessor")
            .field("source", &self.source)
            .field("target", &self.target)
            .field("input_frames_per_block", &self.input_frames_per_block)
            .field("max_output_frames", &self.max_output_frames)
            .field("resampling", &self.resampler.is_some())
            .finish()
    }
}

impl AudioPreprocessor {
    pub fn new(source: AudioFormat, target: AudioFormat) -> Result<Self, ProcessingError> {
        Self::with_chunk_frames(source, target, DEFAULT_CHUNK_FRAMES)
    }

    pub fn with_chunk_frames(
        source: AudioFormat,
        target: AudioFormat,
        chunk_frames: usize,
    ) -> Result<Self, ProcessingError> {
        if chunk_frames == 0 {
            return Err(ProcessingError::InvalidChunkSize);
        }

        validate_channel_conversion(source.channels, target.channels)?;
        let processing_channels = usize::from(target.channels);

        let mut resampler = if source.sample_rate_hz == target.sample_rate_hz {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    usize::try_from(source.sample_rate_hz)
                        .map_err(|_| ProcessingError::InvalidFormat)?,
                    usize::try_from(target.sample_rate_hz)
                        .map_err(|_| ProcessingError::InvalidFormat)?,
                    chunk_frames,
                    processing_channels,
                    FixedSync::Input,
                )
                .map_err(|error| ProcessingError::ResamplerConstruction(error.to_string()))?,
            )
        };

        let input_frames_per_block = resampler
            .as_ref()
            .map_or(chunk_frames, Resampler::input_frames_next);
        let max_output_frames = resampler
            .as_ref()
            .map_or(input_frames_per_block, Resampler::output_frames_max);
        let normalized_samples = input_frames_per_block
            .checked_mul(processing_channels)
            .ok_or(ProcessingError::InvalidChunkSize)?;

        if let Some(resampler) = &mut resampler {
            resampler.reset();
        }

        Ok(Self {
            source,
            target,
            input_frames_per_block,
            normalized_input: vec![0.0; normalized_samples],
            resampler,
            max_output_frames,
        })
    }

    #[must_use]
    pub fn source_format(&self) -> AudioFormat {
        self.source
    }

    #[must_use]
    pub fn target_format(&self) -> AudioFormat {
        self.target
    }

    #[must_use]
    pub fn input_frames_per_block(&self) -> usize {
        self.input_frames_per_block
    }

    #[must_use]
    pub fn required_input_samples(&self) -> usize {
        self.input_frames_per_block * usize::from(self.source.channels)
    }

    #[must_use]
    pub fn max_output_frames(&self) -> usize {
        self.max_output_frames
    }

    #[must_use]
    pub fn max_output_samples(&self) -> usize {
        self.max_output_frames * usize::from(self.target.channels)
    }

    pub fn reset(&mut self) {
        if let Some(resampler) = &mut self.resampler {
            resampler.reset();
        }
    }

    /// Process exactly one reusable streaming block.
    ///
    /// The caller is responsible for collecting complete source frames until
    /// `required_input_samples()` samples are available. Final partial-block flushing
    /// is intentionally a separate orchestration concern so an utterance boundary is
    /// never guessed inside this DSP primitive.
    pub fn process_block(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<ProcessReport, ProcessingError> {
        let required_input = self.required_input_samples();
        if input.len() != required_input {
            return Err(ProcessingError::InvalidInputLength {
                expected: required_input,
                actual: input.len(),
            });
        }

        let required_output = self.max_output_samples();
        if output.len() < required_output {
            return Err(ProcessingError::OutputTooSmall {
                required: required_output,
                actual: output.len(),
            });
        }

        normalize_channels(
            input,
            usize::from(self.source.channels),
            usize::from(self.target.channels),
            &mut self.normalized_input,
        )?;

        let target_channels = usize::from(self.target.channels);
        if let Some(resampler) = &mut self.resampler {
            let input_adapter = InterleavedSlice::new(
                &self.normalized_input,
                target_channels,
                self.input_frames_per_block,
            )
            .map_err(|error| ProcessingError::Adapter(error.to_string()))?;
            let mut output_adapter =
                InterleavedSlice::new_mut(output, target_channels, output.len() / target_channels)
                    .map_err(|error| ProcessingError::Adapter(error.to_string()))?;

            let (input_frames, output_frames) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|error| ProcessingError::Resampling(error.to_string()))?;

            Ok(ProcessReport {
                input_frames,
                output_frames,
            })
        } else {
            let samples = self.normalized_input.len();
            output[..samples].copy_from_slice(&self.normalized_input);
            Ok(ProcessReport {
                input_frames: self.input_frames_per_block,
                output_frames: self.input_frames_per_block,
            })
        }
    }
}

fn validate_channel_conversion(source: u16, target: u16) -> Result<(), ProcessingError> {
    if source == target || target == 1 {
        Ok(())
    } else {
        Err(ProcessingError::UnsupportedChannelConversion { source, target })
    }
}

fn normalize_channels(
    input: &[f32],
    source_channels: usize,
    target_channels: usize,
    output: &mut [f32],
) -> Result<(), ProcessingError> {
    if source_channels == 0 || target_channels == 0 {
        return Err(ProcessingError::InvalidFormat);
    }

    if source_channels == target_channels {
        if output.len() != input.len() {
            return Err(ProcessingError::InvalidInputLength {
                expected: output.len(),
                actual: input.len(),
            });
        }
        output.copy_from_slice(input);
        return Ok(());
    }

    if target_channels != 1 {
        return Err(ProcessingError::UnsupportedChannelConversion {
            source: u16::try_from(source_channels).unwrap_or(u16::MAX),
            target: u16::try_from(target_channels).unwrap_or(u16::MAX),
        });
    }

    let mut frames = input.chunks_exact(source_channels);
    if output.len() != frames.len() {
        return Err(ProcessingError::InvalidInputLength {
            expected: output.len() * source_channels,
            actual: input.len(),
        });
    }

    for (destination, frame) in output.iter_mut().zip(frames.by_ref()) {
        let sum = frame.iter().copied().sum::<f32>();
        *destination = sum / source_channels as f32;
    }

    if !frames.remainder().is_empty() {
        return Err(ProcessingError::InvalidInputLength {
            expected: output.len() * source_channels,
            actual: input.len(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use blcvoice_audio::AudioSampleFormat;

    fn format(channels: u16, sample_rate_hz: u32) -> AudioFormat {
        AudioFormat::new(channels, sample_rate_hz).expect("test format must be valid")
    }

    #[test]
    fn rejects_invalid_signal_shapes() {
        assert_eq!(
            AudioFormat::new(0, 48_000),
            Err(ProcessingError::InvalidFormat)
        );
        assert_eq!(AudioFormat::new(1, 0), Err(ProcessingError::InvalidFormat));
    }

    #[test]
    fn derives_processing_shape_from_native_stream_config() {
        let native = AudioStreamConfig {
            channels: 2,
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::I16,
        };

        assert_eq!(
            AudioFormat::from_stream_config(&native),
            Ok(format(2, 48_000))
        );
    }

    #[test]
    fn identity_path_preserves_interleaved_samples() {
        let signal = format(2, 48_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(signal, signal, 2).expect("valid processor");
        let input = [0.25, -0.25, 0.5, -0.5];
        let mut output = [0.0; 4];

        let report = processor
            .process_block(&input, &mut output)
            .expect("identity processing must succeed");

        assert_eq!(output, input);
        assert_eq!(
            report,
            ProcessReport {
                input_frames: 2,
                output_frames: 2,
            }
        );
    }

    #[test]
    fn stereo_to_mono_uses_uniform_frame_average() {
        let source = format(2, 48_000);
        let target = format(1, 48_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 3).expect("valid processor");
        let input = [1.0, -1.0, 0.5, 0.5, -0.25, 0.75];
        let mut output = [0.0; 3];

        processor
            .process_block(&input, &mut output)
            .expect("downmix must succeed");

        assert_eq!(output, [0.0, 0.5, 0.25]);
    }

    #[test]
    fn refuses_to_invent_multichannel_upmix_policy() {
        let result =
            AudioPreprocessor::with_chunk_frames(format(1, 48_000), format(2, 48_000), 128);

        assert!(matches!(
            result,
            Err(ProcessingError::UnsupportedChannelConversion {
                source: 1,
                target: 2
            })
        ));
    }

    #[test]
    fn fixed_rate_resampler_exposes_stable_preallocated_bounds() {
        let source = format(1, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let input = vec![0.25; processor.required_input_samples()];
        let mut output = vec![0.0; processor.max_output_samples()];

        let report = processor
            .process_block(&input, &mut output)
            .expect("resampling must succeed");

        assert_eq!(report.input_frames, processor.input_frames_per_block());
        assert!(report.output_frames > 0);
        assert!(report.output_frames <= processor.max_output_frames());
        assert!(
            output[..report.output_frames]
                .iter()
                .all(|sample| sample.is_finite())
        );
    }

    #[test]
    fn rejects_short_input_and_output_buffers() {
        let source = format(1, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let input = vec![0.0; processor.required_input_samples()];
        let mut output = vec![0.0; processor.max_output_samples()];

        assert!(matches!(
            processor.process_block(&input[..input.len() - 1], &mut output),
            Err(ProcessingError::InvalidInputLength { .. })
        ));

        let mut too_small = vec![0.0; processor.max_output_samples() - 1];
        assert!(matches!(
            processor.process_block(&input, &mut too_small),
            Err(ProcessingError::OutputTooSmall { .. })
        ));
    }
}
