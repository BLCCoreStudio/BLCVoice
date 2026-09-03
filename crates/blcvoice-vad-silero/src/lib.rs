#![forbid(unsafe_code)]

use std::fmt;

use blcvoice_vad::{
    SpeechRange, VadAnalysis, VadConfig, VadError, VoiceActivityDetector, validate_mono_samples,
};
use silero_vad_crs::{SileroVad, TimestampConfig, get_timestamps_from_probs_with_config};

pub struct SileroVoiceActivityDetector {
    model: Option<(u32, SileroVad)>,
}

impl Default for SileroVoiceActivityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SileroVoiceActivityDetector {
    #[must_use]
    pub const fn new() -> Self {
        Self { model: None }
    }

    fn model_for_rate(&mut self, sample_rate_hz: u32) -> Result<&mut SileroVad, VadError> {
        let rate_changed = self
            .model
            .as_ref()
            .is_none_or(|(rate, _)| *rate != sample_rate_hz);
        if rate_changed {
            let rate = usize::try_from(sample_rate_hz).map_err(|_| VadError::InvalidSampleRate)?;
            let model = SileroVad::with_sample_rate(rate)
                .map_err(|error| VadError::Backend(error.to_string()))?;
            self.model = Some((sample_rate_hz, model));
        }
        self.model
            .as_mut()
            .map(|(_, model)| model)
            .ok_or_else(|| VadError::Backend("Silero VAD model was not initialized".to_owned()))
    }
}

impl fmt::Debug for SileroVoiceActivityDetector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SileroVoiceActivityDetector")
            .field(
                "sample_rate_hz",
                &self.model.as_ref().map(|(rate, _)| *rate),
            )
            .finish_non_exhaustive()
    }
}

impl VoiceActivityDetector for SileroVoiceActivityDetector {
    fn backend_name(&self) -> &'static str {
        "silero-vad-crs"
    }

    fn analyze_mono(
        &mut self,
        samples: &[f32],
        sample_rate_hz: u32,
        config: VadConfig,
    ) -> Result<VadAnalysis, VadError> {
        validate_mono_samples(samples, sample_rate_hz)?;
        let config = config.validate()?;
        if samples.is_empty() {
            return VadAnalysis::new(sample_rate_hz, 0, Vec::new(), None);
        }

        let model = self.model_for_rate(sample_rate_hz)?;
        let probabilities = model
            .forward_audio(samples)
            .map_err(|error| VadError::Backend(error.to_string()))?;
        let max_probability = probabilities.iter().copied().reduce(f32::max);
        let timestamps = get_timestamps_from_probs_with_config(
            &probabilities,
            samples.len(),
            TimestampConfig {
                sampling_rate: usize::try_from(sample_rate_hz)
                    .map_err(|_| VadError::InvalidSampleRate)?,
                threshold: config.threshold,
                min_speech_duration_ms: usize::try_from(config.min_speech_duration_ms).map_err(
                    |_| VadError::InvalidConfiguration("minimum speech duration is too large"),
                )?,
                min_silence_duration_ms: usize::try_from(config.min_silence_duration_ms).map_err(
                    |_| VadError::InvalidConfiguration("minimum silence duration is too large"),
                )?,
                speech_pad_ms: usize::try_from(config.speech_pad_ms)
                    .map_err(|_| VadError::InvalidConfiguration("speech padding is too large"))?,
                window_size_samples: model.source_window_samples(),
                ..TimestampConfig::default()
            },
        );

        let ranges = timestamps
            .into_iter()
            .map(|timestamp| SpeechRange::new(timestamp.start, timestamp.end))
            .collect::<Result<Vec<_>, _>>()?;
        VadAnalysis::new(sample_rate_hz, samples.len(), ranges, max_probability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_second_of_silence_contains_no_speech() {
        let mut detector = SileroVoiceActivityDetector::new();
        let analysis = detector
            .analyze_mono(&vec![0.0; 16_000], 16_000, VadConfig::default())
            .expect("silence analysis must succeed");

        assert!(!analysis.contains_speech());
        assert_eq!(analysis.analyzed_samples, 16_000);
    }

    #[test]
    fn detector_rebuilds_cleanly_for_a_different_sample_rate() {
        let mut detector = SileroVoiceActivityDetector::new();
        detector
            .analyze_mono(&vec![0.0; 16_000], 16_000, VadConfig::default())
            .unwrap();
        detector
            .analyze_mono(&vec![0.0; 48_000], 48_000, VadConfig::default())
            .unwrap();

        assert_eq!(detector.model.as_ref().map(|(rate, _)| *rate), Some(48_000));
    }
}
