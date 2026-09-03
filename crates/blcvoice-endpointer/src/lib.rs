#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

pub const MIN_LEVEL_DBFS: f32 = -120.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveEndpointerConfig {
    pub minimum_start_dbfs: f32,
    pub minimum_continue_dbfs: f32,
    pub start_snr_db: f32,
    pub continue_snr_db: f32,
    pub minimum_speech_ms: u32,
    pub trailing_silence_ms: u32,
    pub noise_floor_alpha: f32,
    pub initial_noise_floor_dbfs: f32,
}

impl Default for AdaptiveEndpointerConfig {
    fn default() -> Self {
        Self {
            minimum_start_dbfs: -45.0,
            minimum_continue_dbfs: -50.0,
            start_snr_db: 12.0,
            continue_snr_db: 6.0,
            minimum_speech_ms: 120,
            trailing_silence_ms: 800,
            noise_floor_alpha: 0.05,
            initial_noise_floor_dbfs: -60.0,
        }
    }
}

impl AdaptiveEndpointerConfig {
    pub fn validate(self) -> Result<Self, EndpointerError> {
        let finite = self.minimum_start_dbfs.is_finite()
            && self.minimum_continue_dbfs.is_finite()
            && self.start_snr_db.is_finite()
            && self.continue_snr_db.is_finite()
            && self.noise_floor_alpha.is_finite()
            && self.initial_noise_floor_dbfs.is_finite();
        if !finite {
            return Err(EndpointerError::InvalidConfiguration(
                "endpointer thresholds must be finite",
            ));
        }
        if self.minimum_speech_ms == 0 || self.trailing_silence_ms == 0 {
            return Err(EndpointerError::InvalidConfiguration(
                "endpointer speech and silence durations must be non-zero",
            ));
        }
        if !(0.0..=1.0).contains(&self.noise_floor_alpha) {
            return Err(EndpointerError::InvalidConfiguration(
                "endpointer noise-floor alpha must be between zero and one",
            ));
        }
        if self.minimum_continue_dbfs > self.minimum_start_dbfs {
            return Err(EndpointerError::InvalidConfiguration(
                "continue threshold must not be stricter than start threshold",
            ));
        }
        if self.continue_snr_db > self.start_snr_db {
            return Err(EndpointerError::InvalidConfiguration(
                "continue SNR must not be stricter than start SNR",
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelObservation {
    pub level_dbfs: f32,
    pub duration_ms: u32,
}

impl LevelObservation {
    pub fn new(level_dbfs: f32, duration_ms: u32) -> Result<Self, EndpointerError> {
        if !level_dbfs.is_finite() {
            return Err(EndpointerError::InvalidObservation(
                "observed audio level must be finite",
            ));
        }
        if duration_ms == 0 {
            return Err(EndpointerError::InvalidObservation(
                "observed audio duration must be non-zero",
            ));
        }
        Ok(Self {
            level_dbfs,
            duration_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointerDecision {
    Continue,
    SpeechStarted,
    EndOfSpeech,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EndpointerSnapshot {
    pub speech_started: bool,
    pub ended: bool,
    pub noise_floor_dbfs: f32,
    pub start_threshold_dbfs: f32,
    pub continue_threshold_dbfs: f32,
    pub candidate_speech_ms: u32,
    pub trailing_silence_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Waiting,
    Speech,
    Ended,
}

#[derive(Debug)]
pub struct AdaptiveEndpointer {
    config: AdaptiveEndpointerConfig,
    phase: Phase,
    noise_floor_dbfs: f32,
    candidate_speech_ms: u32,
    trailing_silence_ms: u32,
}

impl AdaptiveEndpointer {
    pub fn new(config: AdaptiveEndpointerConfig) -> Result<Self, EndpointerError> {
        let config = config.validate()?;
        Ok(Self {
            noise_floor_dbfs: config.initial_noise_floor_dbfs,
            config,
            phase: Phase::Waiting,
            candidate_speech_ms: 0,
            trailing_silence_ms: 0,
        })
    }

    pub fn observe(
        &mut self,
        observation: LevelObservation,
    ) -> Result<EndpointerDecision, EndpointerError> {
        if !observation.level_dbfs.is_finite() || observation.duration_ms == 0 {
            return Err(EndpointerError::InvalidObservation(
                "endpointer observation is invalid",
            ));
        }

        match self.phase {
            Phase::Ended => Ok(EndpointerDecision::EndOfSpeech),
            Phase::Waiting => self.observe_waiting(observation),
            Phase::Speech => self.observe_speech(observation),
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> EndpointerSnapshot {
        EndpointerSnapshot {
            speech_started: self.phase != Phase::Waiting,
            ended: self.phase == Phase::Ended,
            noise_floor_dbfs: self.noise_floor_dbfs,
            start_threshold_dbfs: self.start_threshold_dbfs(),
            continue_threshold_dbfs: self.continue_threshold_dbfs(),
            candidate_speech_ms: self.candidate_speech_ms,
            trailing_silence_ms: self.trailing_silence_ms,
        }
    }

    pub fn reset(&mut self) {
        self.phase = Phase::Waiting;
        self.noise_floor_dbfs = self.config.initial_noise_floor_dbfs;
        self.candidate_speech_ms = 0;
        self.trailing_silence_ms = 0;
    }

    fn observe_waiting(
        &mut self,
        observation: LevelObservation,
    ) -> Result<EndpointerDecision, EndpointerError> {
        if observation.level_dbfs >= self.start_threshold_dbfs() {
            self.candidate_speech_ms = checked_add_ms(
                self.candidate_speech_ms,
                observation.duration_ms,
                "candidate speech duration overflowed",
            )?;
            if self.candidate_speech_ms >= self.config.minimum_speech_ms {
                self.phase = Phase::Speech;
                self.trailing_silence_ms = 0;
                return Ok(EndpointerDecision::SpeechStarted);
            }
            return Ok(EndpointerDecision::Continue);
        }

        self.candidate_speech_ms = 0;
        self.update_noise_floor(observation.level_dbfs);
        Ok(EndpointerDecision::Continue)
    }

    fn observe_speech(
        &mut self,
        observation: LevelObservation,
    ) -> Result<EndpointerDecision, EndpointerError> {
        if observation.level_dbfs >= self.continue_threshold_dbfs() {
            self.trailing_silence_ms = 0;
            return Ok(EndpointerDecision::Continue);
        }

        self.trailing_silence_ms = checked_add_ms(
            self.trailing_silence_ms,
            observation.duration_ms,
            "trailing silence duration overflowed",
        )?;
        if self.trailing_silence_ms >= self.config.trailing_silence_ms {
            self.phase = Phase::Ended;
            return Ok(EndpointerDecision::EndOfSpeech);
        }
        Ok(EndpointerDecision::Continue)
    }

    fn update_noise_floor(&mut self, observed_dbfs: f32) {
        let alpha = self.config.noise_floor_alpha;
        self.noise_floor_dbfs = (1.0 - alpha).mul_add(self.noise_floor_dbfs, alpha * observed_dbfs);
    }

    fn start_threshold_dbfs(&self) -> f32 {
        self.config
            .minimum_start_dbfs
            .max(self.noise_floor_dbfs + self.config.start_snr_db)
    }

    fn continue_threshold_dbfs(&self) -> f32 {
        self.config
            .minimum_continue_dbfs
            .max(self.noise_floor_dbfs + self.config.continue_snr_db)
    }
}

fn checked_add_ms(current: u32, additional: u32, message: &'static str) -> Result<u32, EndpointerError> {
    current
        .checked_add(additional)
        .ok_or(EndpointerError::InvalidObservation(message))
}

#[must_use]
pub fn rms_dbfs(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return MIN_LEVEL_DBFS;
    }

    let mut sum_squares = 0.0_f64;
    let mut finite_samples = 0_u64;
    for &sample in samples {
        if sample.is_finite() {
            let sample = f64::from(sample);
            sum_squares += sample * sample;
            finite_samples += 1;
        }
    }
    if finite_samples == 0 || sum_squares <= f64::EPSILON {
        return MIN_LEVEL_DBFS;
    }

    let rms = (sum_squares / finite_samples as f64).sqrt();
    let dbfs = 20.0 * rms.log10();
    dbfs.max(f64::from(MIN_LEVEL_DBFS)) as f32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointerError {
    InvalidConfiguration(&'static str),
    InvalidObservation(&'static str),
}

impl fmt::Display for EndpointerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) | Self::InvalidObservation(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl Error for EndpointerError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(level_dbfs: f32, duration_ms: u32) -> LevelObservation {
        LevelObservation::new(level_dbfs, duration_ms).expect("test observation must be valid")
    }

    #[test]
    fn sustained_speech_then_silence_reaches_endpoint() {
        let mut endpointer = AdaptiveEndpointer::new(AdaptiveEndpointerConfig::default())
            .expect("default config must be valid");

        assert_eq!(
            endpointer.observe(observation(-25.0, 60)).unwrap(),
            EndpointerDecision::Continue
        );
        assert_eq!(
            endpointer.observe(observation(-24.0, 60)).unwrap(),
            EndpointerDecision::SpeechStarted
        );
        assert_eq!(
            endpointer.observe(observation(-60.0, 400)).unwrap(),
            EndpointerDecision::Continue
        );
        assert_eq!(
            endpointer.observe(observation(-61.0, 400)).unwrap(),
            EndpointerDecision::EndOfSpeech
        );
        assert!(endpointer.snapshot().ended);
    }

    #[test]
    fn short_noise_burst_does_not_start_speech() {
        let mut endpointer = AdaptiveEndpointer::new(AdaptiveEndpointerConfig::default()).unwrap();
        assert_eq!(
            endpointer.observe(observation(-20.0, 50)).unwrap(),
            EndpointerDecision::Continue
        );
        assert_eq!(
            endpointer.observe(observation(-70.0, 50)).unwrap(),
            EndpointerDecision::Continue
        );
        assert!(!endpointer.snapshot().speech_started);
    }

    #[test]
    fn noise_floor_raises_dynamic_threshold() {
        let mut endpointer = AdaptiveEndpointer::new(AdaptiveEndpointerConfig {
            noise_floor_alpha: 1.0,
            ..AdaptiveEndpointerConfig::default()
        })
        .unwrap();
        endpointer.observe(observation(-42.0, 100)).unwrap();
        let snapshot = endpointer.snapshot();
        assert_eq!(snapshot.noise_floor_dbfs, -42.0);
        assert_eq!(snapshot.start_threshold_dbfs, -30.0);
    }

    #[test]
    fn reset_restores_initial_state() {
        let mut endpointer = AdaptiveEndpointer::new(AdaptiveEndpointerConfig::default()).unwrap();
        endpointer.observe(observation(-20.0, 120)).unwrap();
        assert!(endpointer.snapshot().speech_started);
        endpointer.reset();
        let snapshot = endpointer.snapshot();
        assert!(!snapshot.speech_started);
        assert!(!snapshot.ended);
        assert_eq!(snapshot.noise_floor_dbfs, -60.0);
    }

    #[test]
    fn rms_dbfs_handles_silence_and_full_scale() {
        assert_eq!(rms_dbfs(&[]), MIN_LEVEL_DBFS);
        assert_eq!(rms_dbfs(&[0.0, 0.0]), MIN_LEVEL_DBFS);
        assert!((rms_dbfs(&[1.0, -1.0]) - 0.0).abs() < 0.001);
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let error = AdaptiveEndpointerConfig {
            trailing_silence_ms: 0,
            ..AdaptiveEndpointerConfig::default()
        }
        .validate()
        .unwrap_err();
        assert_eq!(
            error,
            EndpointerError::InvalidConfiguration(
                "endpointer speech and silence durations must be non-zero"
            )
        );
    }
}
