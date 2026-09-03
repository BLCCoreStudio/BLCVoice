#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadConfig {
    pub threshold: f32,
    pub min_speech_duration_ms: u32,
    pub min_silence_duration_ms: u32,
    pub speech_pad_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_speech_duration_ms: 250,
            min_silence_duration_ms: 100,
            speech_pad_ms: 60,
        }
    }
}

impl VadConfig {
    pub fn validate(self) -> Result<Self, VadError> {
        if !self.threshold.is_finite() || !(0.0..=1.0).contains(&self.threshold) {
            return Err(VadError::InvalidConfiguration(
                "VAD threshold must be finite and between 0 and 1",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechRange {
    pub start_sample: usize,
    pub end_sample: usize,
}

impl SpeechRange {
    pub fn new(start_sample: usize, end_sample: usize) -> Result<Self, VadError> {
        if start_sample >= end_sample {
            return Err(VadError::InvalidRange {
                start_sample,
                end_sample,
            });
        }
        Ok(Self {
            start_sample,
            end_sample,
        })
    }

    #[must_use]
    pub const fn sample_len(self) -> usize {
        self.end_sample - self.start_sample
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VadAnalysis {
    pub sample_rate_hz: u32,
    pub analyzed_samples: usize,
    pub speech_ranges: Vec<SpeechRange>,
    pub max_speech_probability: Option<f32>,
}

impl VadAnalysis {
    pub fn new(
        sample_rate_hz: u32,
        analyzed_samples: usize,
        speech_ranges: Vec<SpeechRange>,
        max_speech_probability: Option<f32>,
    ) -> Result<Self, VadError> {
        if sample_rate_hz == 0 {
            return Err(VadError::InvalidSampleRate);
        }
        if let Some(probability) = max_speech_probability
            && (!probability.is_finite() || !(0.0..=1.0).contains(&probability))
        {
            return Err(VadError::InvalidProbability(probability));
        }
        for range in &speech_ranges {
            if range.start_sample >= range.end_sample || range.end_sample > analyzed_samples {
                return Err(VadError::InvalidRange {
                    start_sample: range.start_sample,
                    end_sample: range.end_sample,
                });
            }
        }
        Ok(Self {
            sample_rate_hz,
            analyzed_samples,
            speech_ranges,
            max_speech_probability,
        })
    }

    #[must_use]
    pub fn contains_speech(&self) -> bool {
        !self.speech_ranges.is_empty()
    }

    #[must_use]
    pub fn speech_envelope(&self) -> Option<SpeechRange> {
        let first = self.speech_ranges.first()?;
        let last = self.speech_ranges.last()?;
        Some(SpeechRange {
            start_sample: first.start_sample,
            end_sample: last.end_sample,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VadError {
    InvalidConfiguration(&'static str),
    InvalidSampleRate,
    NonFiniteSample {
        index: usize,
    },
    InvalidRange {
        start_sample: usize,
        end_sample: usize,
    },
    InvalidProbability(f32),
    Backend(String),
}

impl fmt::Display for VadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => formatter.write_str(message),
            Self::InvalidSampleRate => formatter.write_str("VAD sample rate must be non-zero"),
            Self::NonFiniteSample { index } => {
                write!(formatter, "VAD input sample at index {index} is not finite")
            }
            Self::InvalidRange {
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "invalid VAD speech range {start_sample}..{end_sample}"
            ),
            Self::InvalidProbability(probability) => write!(
                formatter,
                "VAD probability {probability} is not finite or outside [0, 1]"
            ),
            Self::Backend(message) => write!(formatter, "VAD backend failed: {message}"),
        }
    }
}

impl Error for VadError {}

pub trait VoiceActivityDetector {
    fn backend_name(&self) -> &'static str;

    fn analyze_mono(
        &mut self,
        samples: &[f32],
        sample_rate_hz: u32,
        config: VadConfig,
    ) -> Result<VadAnalysis, VadError>;
}

pub fn validate_mono_samples(samples: &[f32], sample_rate_hz: u32) -> Result<(), VadError> {
    if sample_rate_hz == 0 {
        return Err(VadError::InvalidSampleRate);
    }
    for (index, sample) in samples.iter().enumerate() {
        if !sample.is_finite() {
            return Err(VadError::NonFiniteSample { index });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_exposes_outer_speech_envelope_without_erasing_internal_silence() {
        let analysis = VadAnalysis::new(
            16_000,
            10_000,
            vec![
                SpeechRange::new(1_000, 2_000).unwrap(),
                SpeechRange::new(4_000, 6_000).unwrap(),
            ],
            Some(0.9),
        )
        .unwrap();

        assert_eq!(
            analysis.speech_envelope(),
            Some(SpeechRange {
                start_sample: 1_000,
                end_sample: 6_000,
            })
        );
    }

    #[test]
    fn invalid_threshold_is_rejected() {
        assert!(
            VadConfig {
                threshold: 1.1,
                ..VadConfig::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn non_finite_audio_is_rejected() {
        assert!(matches!(
            validate_mono_samples(&[0.0, f32::NAN], 16_000),
            Err(VadError::NonFiniteSample { index: 1 })
        ));
    }
}
