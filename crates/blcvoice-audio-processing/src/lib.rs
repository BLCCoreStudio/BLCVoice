#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

use audioadapter_buffers::direct::InterleavedSlice;
use blcvoice_audio::AudioStreamConfig;
use rubato::{Fft, FixedSync, Indexing, Resampler};

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
    InvalidUtteranceLimit,
    UnsupportedChannelConversion {
        source: u16,
        target: u16,
    },
    InvalidInputLength {
        expected: usize,
        actual: usize,
    },
    UnalignedInput {
        channels: u16,
        samples: usize,
    },
    UtteranceTooLong {
        max_frames: usize,
        attempted_frames: usize,
    },
    OutputTooSmall {
        required: usize,
        actual: usize,
    },
    BufferSizeOverflow,
    ResourceExhausted,
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
            Self::InvalidUtteranceLimit => {
                formatter.write_str("utterance limit must allow at least one source frame")
            }
            Self::UnsupportedChannelConversion { source, target } => write!(
                formatter,
                "unsupported channel conversion from {source} channels to {target} channels"
            ),
            Self::InvalidInputLength { expected, actual } => write!(
                formatter,
                "audio preprocessing expected {expected} input samples but received {actual}"
            ),
            Self::UnalignedInput { channels, samples } => write!(
                formatter,
                "{samples} interleaved samples do not form complete {channels}-channel frames"
            ),
            Self::UtteranceTooLong {
                max_frames,
                attempted_frames,
            } => write!(
                formatter,
                "utterance would contain {attempted_frames} frames, exceeding the configured {max_frames}-frame limit"
            ),
            Self::OutputTooSmall { required, actual } => write!(
                formatter,
                "audio preprocessing requires at least {required} output samples but received {actual}"
            ),
            Self::BufferSizeOverflow => formatter.write_str("audio buffer size overflow"),
            Self::ResourceExhausted => {
                formatter.write_str("audio preprocessing could not reserve required memory")
            }
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

/// Exact processed audio for one completed utterance.
///
/// The samples borrow reusable storage owned by the processor. Starting another
/// utterance-processing call requires this value to be dropped first, preventing the
/// backing buffer from being mutated while a caller still uses it.
#[derive(Debug, PartialEq)]
pub struct ProcessedAudio<'a> {
    samples: &'a [f32],
    format: AudioFormat,
    frames: usize,
}

impl<'a> ProcessedAudio<'a> {
    #[must_use]
    pub fn samples(&self) -> &'a [f32] {
        self.samples
    }

    #[must_use]
    pub fn format(&self) -> AudioFormat {
        self.format
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.frames
    }
}

/// Bounded worker-side accumulator for one device-native utterance.
///
/// `push_interleaved` may reserve memory and therefore must never be called from the
/// CPAL realtime callback. The capture consumer/worker owns this buffer.
#[derive(Debug, Clone)]
pub struct UtteranceBuffer {
    format: AudioFormat,
    max_frames: usize,
    samples: Vec<f32>,
}

impl UtteranceBuffer {
    pub fn new(format: AudioFormat, max_frames: usize) -> Result<Self, ProcessingError> {
        if max_frames == 0 {
            return Err(ProcessingError::InvalidUtteranceLimit);
        }

        Ok(Self {
            format,
            max_frames,
            samples: Vec::new(),
        })
    }

    pub fn with_max_duration_ms(
        format: AudioFormat,
        max_duration_ms: u32,
    ) -> Result<Self, ProcessingError> {
        if max_duration_ms == 0 {
            return Err(ProcessingError::InvalidUtteranceLimit);
        }

        let frames = u64::from(format.sample_rate_hz)
            .checked_mul(u64::from(max_duration_ms))
            .ok_or(ProcessingError::BufferSizeOverflow)?
            .div_ceil(1_000);
        let max_frames =
            usize::try_from(frames).map_err(|_| ProcessingError::BufferSizeOverflow)?;
        Self::new(format, max_frames)
    }

    #[must_use]
    pub fn format(&self) -> AudioFormat {
        self.format
    }

    #[must_use]
    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.format.channels)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    #[must_use]
    pub fn as_interleaved(&self) -> &[f32] {
        &self.samples
    }

    pub fn push_interleaved(&mut self, input: &[f32]) -> Result<(), ProcessingError> {
        let channels = usize::from(self.format.channels);
        if !input.len().is_multiple_of(channels) {
            return Err(ProcessingError::UnalignedInput {
                channels: self.format.channels,
                samples: input.len(),
            });
        }

        let incoming_frames = input.len() / channels;
        let attempted_frames = self
            .frames()
            .checked_add(incoming_frames)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        if attempted_frames > self.max_frames {
            return Err(ProcessingError::UtteranceTooLong {
                max_frames: self.max_frames,
                attempted_frames,
            });
        }

        self.samples
            .try_reserve(input.len())
            .map_err(|_| ProcessingError::ResourceExhausted)?;
        self.samples.extend_from_slice(input);
        Ok(())
    }

    /// Clear the utterance while retaining allocated capacity for the next dictation.
    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Reusable worker-side preprocessing stage between native capture and an ASR engine.
///
/// The stage never runs in the CPAL callback. Construction owns reusable scratch
/// storage and the resampler instance. `process_block` is the low-level fixed-block
/// primitive for future incremental processing, while `process_utterance` is the safe
/// v0.1 path for completed push-to-talk utterances and handles final partial audio plus
/// resampler delay trimming/flushing through Rubato's whole-clip API.
pub struct AudioPreprocessor {
    source: AudioFormat,
    target: AudioFormat,
    input_frames_per_block: usize,
    normalized_input: Vec<f32>,
    utterance_input: Vec<f32>,
    utterance_output: Vec<f32>,
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
            .field(
                "utterance_scratch_capacity",
                &self.utterance_input.capacity(),
            )
            .field(
                "utterance_output_capacity",
                &self.utterance_output.capacity(),
            )
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
            .ok_or(ProcessingError::BufferSizeOverflow)?;

        if let Some(resampler) = &mut resampler {
            resampler.reset();
        }

        Ok(Self {
            source,
            target,
            input_frames_per_block,
            normalized_input: vec![0.0; normalized_samples],
            utterance_input: Vec::new(),
            utterance_output: Vec::new(),
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
        self.utterance_input.clear();
        self.utterance_output.clear();
    }

    /// Process exactly one reusable streaming block.
    ///
    /// The caller is responsible for collecting complete source frames until
    /// `required_input_samples()` samples are available. This primitive intentionally
    /// does not perform utterance-end flushing; use `process_utterance` for completed
    /// push-to-talk clips.
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
            let output_capacity_frames = output.len() / target_channels;
            let mut output_adapter =
                InterleavedSlice::new_mut(output, target_channels, output_capacity_frames)
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

    /// Process a completed utterance of any frame-aligned length.
    ///
    /// This worker-side path explicitly drives Rubato's preallocated block API when
    /// resampling is required. BLCVoice retains and trims the reported startup delay,
    /// processes a short final chunk, then pumps silence only until the exact target
    /// duration is available. Internal vectors grow on demand and retain capacity.
    pub fn process_utterance<'a>(
        &'a mut self,
        input: &[f32],
    ) -> Result<ProcessedAudio<'a>, ProcessingError> {
        let source_channels = usize::from(self.source.channels);
        if !input.len().is_multiple_of(source_channels) {
            return Err(ProcessingError::UnalignedInput {
                channels: self.source.channels,
                samples: input.len(),
            });
        }

        let input_frames = input.len() / source_channels;
        if input_frames == 0 {
            self.utterance_input.clear();
            self.utterance_output.clear();
            if let Some(resampler) = &mut self.resampler {
                resampler.reset();
            }
            return Ok(ProcessedAudio {
                samples: &self.utterance_output,
                format: self.target,
                frames: 0,
            });
        }

        let target_channels = usize::from(self.target.channels);
        let normalized_samples = input_frames
            .checked_mul(target_channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        resize_reusable(&mut self.utterance_input, normalized_samples)?;
        normalize_channels(
            input,
            source_channels,
            target_channels,
            &mut self.utterance_input,
        )?;

        let output_frames = if let Some(resampler) = &mut self.resampler {
            resample_complete_utterance(
                resampler,
                &self.utterance_input,
                target_channels,
                input_frames,
                &mut self.utterance_output,
            )?
        } else {
            resize_reusable(&mut self.utterance_output, normalized_samples)?;
            self.utterance_output.copy_from_slice(&self.utterance_input);
            input_frames
        };

        let valid_samples = output_frames
            .checked_mul(target_channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        self.utterance_output.truncate(valid_samples);

        Ok(ProcessedAudio {
            samples: &self.utterance_output,
            format: self.target,
            frames: output_frames,
        })
    }
}

fn resample_complete_utterance(
    resampler: &mut Fft<f32>,
    input: &[f32],
    channels: usize,
    input_frames: usize,
    output: &mut Vec<f32>,
) -> Result<usize, ProcessingError> {
    resampler.reset();
    let result = (|| {
        let delay_frames = resampler.output_delay();
        let expected_output_frames =
            (resampler.resample_ratio() * input_frames as f64).ceil() as usize;
        let required_raw_frames = delay_frames
            .checked_add(expected_output_frames)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        let output_capacity_frames = resampler
            .process_all_needed_output_len(input_frames)
            .max(required_raw_frames);
        let output_capacity_samples = output_capacity_frames
            .checked_mul(channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        resize_reusable(output, output_capacity_samples)?;

        let input_adapter = InterleavedSlice::new(input, channels, input_frames)
            .map_err(|error| ProcessingError::Adapter(error.to_string()))?;
        let mut output_adapter =
            InterleavedSlice::new_mut(output, channels, output_capacity_frames)
                .map_err(|error| ProcessingError::Adapter(error.to_string()))?;

        let mut input_offset = 0usize;
        let mut output_offset = 0usize;
        let mut frames_left = input_frames;

        loop {
            let required_input_frames = resampler.input_frames_next();
            if frames_left < required_input_frames {
                break;
            }

            let indexing = Indexing {
                input_offset,
                output_offset,
                partial_len: None,
                active_channels_mask: None,
            };
            let (consumed, produced) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
                .map_err(|error| ProcessingError::Resampling(error.to_string()))?;
            if consumed == 0 {
                return Err(ProcessingError::Resampling(
                    "resampler consumed no input from a complete block".to_owned(),
                ));
            }
            input_offset = input_offset
                .checked_add(consumed)
                .ok_or(ProcessingError::BufferSizeOverflow)?;
            output_offset = output_offset
                .checked_add(produced)
                .ok_or(ProcessingError::BufferSizeOverflow)?;
            frames_left = frames_left
                .checked_sub(consumed)
                .ok_or(ProcessingError::Resampling(
                    "resampler consumed more frames than were available".to_owned(),
                ))?;
        }

        if frames_left > 0 {
            let indexing = Indexing {
                input_offset,
                output_offset,
                partial_len: Some(frames_left),
                active_channels_mask: None,
            };
            let (_consumed, produced) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
                .map_err(|error| ProcessingError::Resampling(error.to_string()))?;
            output_offset = output_offset
                .checked_add(produced)
                .ok_or(ProcessingError::BufferSizeOverflow)?;
        }

        while output_offset < required_raw_frames {
            let indexing = Indexing {
                input_offset: 0,
                output_offset,
                partial_len: Some(0),
                active_channels_mask: None,
            };
            let (_consumed, produced) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
                .map_err(|error| ProcessingError::Resampling(error.to_string()))?;
            if produced == 0 {
                return Err(ProcessingError::Resampling(
                    "resampler produced no output while flushing its delay".to_owned(),
                ));
            }
            output_offset = output_offset
                .checked_add(produced)
                .ok_or(ProcessingError::BufferSizeOverflow)?;
        }

        let valid_start = delay_frames
            .checked_mul(channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        let valid_samples = expected_output_frames
            .checked_mul(channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        let valid_end = valid_start
            .checked_add(valid_samples)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        let produced_samples = output_offset
            .checked_mul(channels)
            .ok_or(ProcessingError::BufferSizeOverflow)?;
        if valid_end > produced_samples || valid_end > output.len() {
            return Err(ProcessingError::Resampling(
                "resampler flush ended before the exact utterance output was available".to_owned(),
            ));
        }

        output.copy_within(valid_start..valid_end, 0);
        output.truncate(valid_samples);
        Ok(expected_output_frames)
    })();
    resampler.reset();
    result
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

fn resize_reusable(buffer: &mut Vec<f32>, len: usize) -> Result<(), ProcessingError> {
    if len > buffer.len() {
        buffer
            .try_reserve(len - buffer.len())
            .map_err(|_| ProcessingError::ResourceExhausted)?;
    }
    buffer.resize(len, 0.0);
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

    #[test]
    fn utterance_buffer_requires_complete_frames_and_enforces_limit() {
        let signal = format(2, 48_000);
        let mut buffer = UtteranceBuffer::new(signal, 2).expect("valid buffer");

        assert!(matches!(
            buffer.push_interleaved(&[1.0, 2.0, 3.0]),
            Err(ProcessingError::UnalignedInput {
                channels: 2,
                samples: 3
            })
        ));

        buffer
            .push_interleaved(&[1.0, 2.0, 3.0, 4.0])
            .expect("two complete frames fit");
        assert_eq!(buffer.frames(), 2);
        assert!(matches!(
            buffer.push_interleaved(&[5.0, 6.0]),
            Err(ProcessingError::UtteranceTooLong {
                max_frames: 2,
                attempted_frames: 3
            })
        ));
    }

    #[test]
    fn utterance_duration_limit_rounds_up_to_source_frames() {
        let buffer = UtteranceBuffer::with_max_duration_ms(format(2, 44_100), 1)
            .expect("valid duration bound");
        assert_eq!(buffer.max_frames(), 45);
    }

    #[test]
    fn clearing_utterance_keeps_buffer_reusable() {
        let mut buffer = UtteranceBuffer::new(format(1, 48_000), 16).expect("valid buffer");
        buffer
            .push_interleaved(&[0.1, 0.2, 0.3])
            .expect("input fits");
        buffer.clear();
        assert!(buffer.is_empty());
        buffer
            .push_interleaved(&[0.4, 0.5])
            .expect("buffer remains reusable");
        assert_eq!(buffer.as_interleaved(), &[0.4, 0.5]);
    }

    #[test]
    fn complete_utterance_identity_path_accepts_non_block_length() {
        let signal = format(2, 48_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(signal, signal, 960).expect("valid processor");
        let input = [0.1, -0.1, 0.2, -0.2, 0.3, -0.3];

        let processed = processor
            .process_utterance(&input)
            .expect("utterance processing must succeed");

        assert_eq!(processed.frames(), 3);
        assert_eq!(processed.format(), signal);
        assert_eq!(processed.samples(), input);
    }

    #[test]
    fn complete_utterance_flushes_final_partial_resampler_chunk() {
        let source = format(1, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let input = vec![0.25; 1_000];

        let processed = processor
            .process_utterance(&input)
            .expect("final partial chunk must be processed");

        assert_eq!(processed.frames(), 334);
        assert_eq!(processed.samples().len(), 334);
        assert!(processed.samples().iter().all(|sample| sample.is_finite()));
        assert!(processed.samples().iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn complete_utterance_handles_audio_shorter_than_resampler_chunk() {
        let source = format(1, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let input = vec![0.5; 100];

        let processed = processor
            .process_utterance(&input)
            .expect("short utterance must be processed");

        assert_eq!(processed.frames(), 34);
        assert_eq!(processed.samples().len(), 34);
        assert!(processed.samples().iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn complete_utterance_combines_downmix_and_resampling() {
        let source = format(2, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let mut input = Vec::with_capacity(2_000);
        for _ in 0..1_000 {
            input.extend_from_slice(&[0.75, 0.25]);
        }

        let processed = processor
            .process_utterance(&input)
            .expect("downmix plus resampling must succeed");

        assert_eq!(processed.frames(), 334);
        assert_eq!(processed.format(), target);
        assert!(processed.samples().iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn complete_utterances_reset_resampler_state_between_calls() {
        let source = format(1, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");
        let input = vec![0.25; 1_000];

        let first = processor
            .process_utterance(&input)
            .expect("first utterance must succeed")
            .samples()
            .to_vec();
        let second = processor
            .process_utterance(&input)
            .expect("second utterance must succeed")
            .samples()
            .to_vec();

        assert_eq!(first, second);
    }

    #[test]
    fn empty_utterance_produces_empty_target_audio() {
        let source = format(2, 48_000);
        let target = format(1, 16_000);
        let mut processor =
            AudioPreprocessor::with_chunk_frames(source, target, 960).expect("valid processor");

        let processed = processor
            .process_utterance(&[])
            .expect("empty utterance is a valid no-op");

        assert_eq!(processed.frames(), 0);
        assert!(processed.samples().is_empty());
        assert_eq!(processed.format(), target);
    }
}
