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
}
