#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// Stable, backend-qualified identifier for an input device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AudioDeviceId(String);

impl AudioDeviceId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidAudioDeviceId> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InvalidAudioDeviceId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AudioDeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAudioDeviceId;

impl fmt::Display for InvalidAudioDeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("audio device id cannot be empty")
    }
}

impl Error for InvalidAudioDeviceId {}

/// Audio host/backend selected by the platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AudioBackend {
    PipeWire,
    PulseAudio,
    Alsa,
    Wasapi,
    CoreAudio,
    Jack,
    Asio,
    AAudio,
    Other(String),
}

impl AudioBackend {
    #[must_use]
    pub fn from_host_name(name: &str) -> Self {
        let normalized: String = name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();

        match normalized.as_str() {
            "pipewire" => Self::PipeWire,
            "pulseaudio" | "pulse" => Self::PulseAudio,
            "alsa" => Self::Alsa,
            "wasapi" => Self::Wasapi,
            "coreaudio" => Self::CoreAudio,
            "jack" => Self::Jack,
            "asio" => Self::Asio,
            "aaudio" => Self::AAudio,
            _ => Self::Other(name.to_owned()),
        }
    }
}

impl fmt::Display for AudioBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PipeWire => formatter.write_str("PipeWire"),
            Self::PulseAudio => formatter.write_str("PulseAudio"),
            Self::Alsa => formatter.write_str("ALSA"),
            Self::Wasapi => formatter.write_str("WASAPI"),
            Self::CoreAudio => formatter.write_str("CoreAudio"),
            Self::Jack => formatter.write_str("JACK"),
            Self::Asio => formatter.write_str("ASIO"),
            Self::AAudio => formatter.write_str("AAudio"),
            Self::Other(name) => formatter.write_str(name),
        }
    }
}

/// Device-native sample representation. Conversion/resampling belongs downstream.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AudioSampleFormat {
    F32,
    F64,
    I8,
    I16,
    I24,
    I32,
    I64,
    U8,
    U16,
    U24,
    U32,
    U64,
    DsdU8,
    DsdU16,
    DsdU32,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStreamConfig {
    pub channels: u16,
    pub sample_rate_hz: u32,
    pub sample_format: AudioSampleFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    pub id: AudioDeviceId,
    pub name: String,
    pub backend: AudioBackend,
    pub is_default: bool,
    pub default_config: Option<AudioStreamConfig>,
}

/// Cross-runtime failure categories used by diagnostics and recovery policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFailureKind {
    NoInputDevices,
    DeviceBusy,
    DeviceChanged,
    DeviceNotAvailable,
    BackendUnavailable,
    InvalidInput,
    PermissionDenied,
    RealtimeDenied,
    ResourceExhausted,
    StreamInvalidated,
    UnsupportedConfig,
    UnsupportedOperation,
    Xrun,
    BackendError,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFailure {
    pub backend: Option<AudioBackend>,
    pub device_id: Option<AudioDeviceId>,
    pub kind: AudioFailureKind,
    pub message: String,
}

impl fmt::Display for AudioFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AudioFailure {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputDiscovery {
    pub selected_backend: Option<AudioBackend>,
    pub devices: Vec<InputDeviceInfo>,
    pub failures: Vec<AudioFailure>,
}

impl InputDiscovery {
    #[must_use]
    pub fn has_usable_input(&self) -> bool {
        self.selected_backend.is_some() && !self.devices.is_empty()
    }
}

/// Runtime adapter contract for discovering microphone/input devices.
pub trait InputDeviceDiscovery: Send + Sync {
    fn discover_input_devices(&self) -> InputDiscovery;
}

/// Bounded handoff capacity between the real-time callback and normal worker code.
///
/// The buffer is deliberately time-based so a backend can size it from the native
/// channel count and sample rate without exposing queue implementation details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureBufferConfig {
    capacity_ms: u32,
}

impl CaptureBufferConfig {
    pub const DEFAULT_CAPACITY_MS: u32 = 1_000;
    pub const MAX_CAPACITY_MS: u32 = 5_000;

    pub fn new(capacity_ms: u32) -> Result<Self, InvalidCaptureBufferConfig> {
        if capacity_ms == 0 || capacity_ms > Self::MAX_CAPACITY_MS {
            return Err(InvalidCaptureBufferConfig);
        }
        Ok(Self { capacity_ms })
    }

    #[must_use]
    pub fn capacity_ms(self) -> u32 {
        self.capacity_ms
    }

    pub fn capacity_samples(
        self,
        stream: &AudioStreamConfig,
    ) -> Result<usize, InvalidCaptureBufferConfig> {
        if stream.channels == 0 || stream.sample_rate_hz == 0 {
            return Err(InvalidCaptureBufferConfig);
        }

        let samples = u64::from(stream.sample_rate_hz)
            .checked_mul(u64::from(stream.channels))
            .and_then(|value| value.checked_mul(u64::from(self.capacity_ms)))
            .map(|value| value.div_ceil(1_000))
            .ok_or(InvalidCaptureBufferConfig)?;

        usize::try_from(samples).map_err(|_| InvalidCaptureBufferConfig)
    }
}

impl Default for CaptureBufferConfig {
    fn default() -> Self {
        Self {
            capacity_ms: Self::DEFAULT_CAPACITY_MS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCaptureBufferConfig;

impl fmt::Display for InvalidCaptureBufferConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("capture buffer duration or stream configuration is invalid")
    }
}

impl Error for InvalidCaptureBufferConfig {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputCaptureRequest {
    pub device_id: AudioDeviceId,
    pub buffer: CaptureBufferConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
    pub received_samples: u64,
    pub dropped_samples: u64,
    pub callback_errors: u64,
    pub last_failure: Option<AudioFailureKind>,
}

/// Active microphone stream exposed to the application without leaking backend types.
///
/// Samples are normalized to `f32` but remain interleaved and at the device-native
/// sample rate/channel count. Downmixing, resampling and VAD happen downstream.
pub trait InputCaptureSession: Send {
    fn stream_config(&self) -> &AudioStreamConfig;

    fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize;

    fn stats(&self) -> CaptureStats;

    fn pause(&self) -> Result<(), AudioFailure>;

    fn resume(&self) -> Result<(), AudioFailure>;
}

/// Runtime adapter contract for starting a bounded, non-blocking microphone capture session.
pub trait InputCaptureFactory: Send + Sync {
    fn start_input_capture(
        &self,
        request: &InputCaptureRequest,
    ) -> Result<Box<dyn InputCaptureSession>, AudioFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_device_ids() {
        assert!(AudioDeviceId::new("  ").is_err());
    }

    #[test]
    fn preserves_backend_qualified_device_id() {
        let id = AudioDeviceId::new("wasapi:{device-guid}").expect("id must be valid");
        assert_eq!(id.as_str(), "wasapi:{device-guid}");
    }

    #[test]
    fn normalizes_known_backend_names() {
        assert_eq!(
            AudioBackend::from_host_name("PipeWire"),
            AudioBackend::PipeWire
        );
        assert_eq!(
            AudioBackend::from_host_name("Pulse Audio"),
            AudioBackend::PulseAudio
        );
        assert_eq!(AudioBackend::from_host_name("WASAPI"), AudioBackend::Wasapi);
        assert_eq!(
            AudioBackend::from_host_name("CoreAudio"),
            AudioBackend::CoreAudio
        );
    }

    #[test]
    fn unknown_backends_remain_visible() {
        assert_eq!(
            AudioBackend::from_host_name("FutureHost"),
            AudioBackend::Other("FutureHost".to_owned())
        );
    }

    #[test]
    fn discovery_requires_backend_and_device() {
        let empty = InputDiscovery::default();
        assert!(!empty.has_usable_input());

        let discovery = InputDiscovery {
            selected_backend: Some(AudioBackend::Wasapi),
            devices: vec![InputDeviceInfo {
                id: AudioDeviceId::new("wasapi:mic").expect("valid id"),
                name: "Microphone".to_owned(),
                backend: AudioBackend::Wasapi,
                is_default: true,
                default_config: None,
            }],
            failures: Vec::new(),
        };
        assert!(discovery.has_usable_input());
    }

    #[test]
    fn capture_buffer_is_sized_from_native_stream_shape() {
        let stream = AudioStreamConfig {
            channels: 2,
            sample_rate_hz: 48_000,
            sample_format: AudioSampleFormat::F32,
        };
        let buffer = CaptureBufferConfig::new(1_000).expect("valid buffer");

        assert_eq!(buffer.capacity_samples(&stream), Ok(96_000));
    }

    #[test]
    fn rejects_zero_and_unbounded_capture_buffers() {
        assert!(CaptureBufferConfig::new(0).is_err());
        assert!(CaptureBufferConfig::new(CaptureBufferConfig::MAX_CAPACITY_MS + 1).is_err());
    }
}
