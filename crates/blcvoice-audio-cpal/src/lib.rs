#![forbid(unsafe_code)]

use blcvoice_audio::{
    AudioBackend, AudioDeviceId, AudioFailure, AudioFailureKind, AudioSampleFormat,
    AudioStreamConfig, InputDeviceDiscovery, InputDeviceInfo, InputDiscovery,
};
use cpal::traits::{DeviceTrait, HostTrait};

/// CPAL-backed input-device discovery with explicit host fallback.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpalInputDeviceDiscovery;

impl InputDeviceDiscovery for CpalInputDeviceDiscovery {
    fn discover_input_devices(&self) -> InputDiscovery {
        let mut failures = Vec::new();
        let mut hosts = cpal::available_hosts();
        hosts.sort_by_key(|host| host_priority(&host.to_string()));

        for host_id in hosts {
            let backend = AudioBackend::from_host_name(&host_id.to_string());
            let host = match cpal::host_from_id(host_id) {
                Ok(host) => host,
                Err(error) => {
                    failures.push(failure(Some(backend), None, &error));
                    continue;
                }
            };

            let default_id = host
                .default_input_device()
                .and_then(|device| device.id().ok())
                .map(|id| id.to_string());

            let devices = match host.devices() {
                Ok(devices) => devices,
                Err(error) => {
                    failures.push(failure(Some(backend), None, &error));
                    continue;
                }
            };

            let mut discovered = Vec::new();
            for device in devices {
                let mut supported_configs = match device.supported_input_configs() {
                    Ok(configs) => configs,
                    Err(error) if error.kind() == cpal::ErrorKind::UnsupportedOperation => continue,
                    Err(error) => {
                        failures.push(failure(Some(backend.clone()), None, &error));
                        continue;
                    }
                };

                if supported_configs.next().is_none() {
                    continue;
                }

                let raw_id = match device.id() {
                    Ok(id) => id,
                    Err(error) => {
                        failures.push(failure(Some(backend.clone()), None, &error));
                        continue;
                    }
                };
                let raw_id_string = raw_id.to_string();
                let device_id = match AudioDeviceId::new(raw_id_string.clone()) {
                    Ok(id) => id,
                    Err(error) => {
                        failures.push(AudioFailure {
                            backend: Some(backend.clone()),
                            device_id: None,
                            kind: AudioFailureKind::Other,
                            message: error.to_string(),
                        });
                        continue;
                    }
                };

                let name = device
                    .description()
                    .map(|description| description.name().to_owned())
                    .unwrap_or_else(|_| device.to_string());

                let default_config = match device.default_input_config() {
                    Ok(config) => Some(AudioStreamConfig {
                        channels: config.channels(),
                        sample_rate_hz: config.sample_rate(),
                        sample_format: map_sample_format(config.sample_format()),
                    }),
                    Err(error) => {
                        failures.push(failure(
                            Some(backend.clone()),
                            Some(device_id.clone()),
                            &error,
                        ));
                        None
                    }
                };

                discovered.push(InputDeviceInfo {
                    id: device_id,
                    name,
                    backend: backend.clone(),
                    is_default: default_id.as_deref() == Some(raw_id_string.as_str()),
                    default_config,
                });
            }

            if !discovered.is_empty() {
                discovered.sort_by(|left, right| {
                    right
                        .is_default
                        .cmp(&left.is_default)
                        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                        .then_with(|| left.id.cmp(&right.id))
                });
                return InputDiscovery {
                    selected_backend: Some(backend),
                    devices: discovered,
                    failures,
                };
            }

            failures.push(AudioFailure {
                backend: Some(backend),
                device_id: None,
                kind: AudioFailureKind::NoInputDevices,
                message: "audio backend exposed no usable input devices".to_owned(),
            });
        }

        InputDiscovery {
            selected_backend: None,
            devices: Vec::new(),
            failures,
        }
    }
}

fn host_priority(name: &str) -> u8 {
    match AudioBackend::from_host_name(name) {
        AudioBackend::PipeWire => 0,
        AudioBackend::PulseAudio => 1,
        AudioBackend::Wasapi | AudioBackend::CoreAudio => 0,
        AudioBackend::Alsa => 2,
        AudioBackend::Jack | AudioBackend::Asio => 3,
        AudioBackend::AAudio => 0,
        AudioBackend::Other(_) => 10,
    }
}

fn failure(backend: Option<AudioBackend>, device_id: Option<AudioDeviceId>, error: &cpal::Error) -> AudioFailure {
    AudioFailure {
        backend,
        device_id,
        kind: map_error_kind(error.kind()),
        message: error.to_string(),
    }
}

fn map_error_kind(kind: cpal::ErrorKind) -> AudioFailureKind {
    match kind {
        cpal::ErrorKind::DeviceBusy => AudioFailureKind::DeviceBusy,
        cpal::ErrorKind::DeviceChanged => AudioFailureKind::DeviceChanged,
        cpal::ErrorKind::DeviceNotAvailable => AudioFailureKind::DeviceNotAvailable,
        cpal::ErrorKind::HostUnavailable => AudioFailureKind::BackendUnavailable,
        cpal::ErrorKind::InvalidInput => AudioFailureKind::InvalidInput,
        cpal::ErrorKind::PermissionDenied => AudioFailureKind::PermissionDenied,
        cpal::ErrorKind::RealtimeDenied => AudioFailureKind::RealtimeDenied,
        cpal::ErrorKind::ResourceExhausted => AudioFailureKind::ResourceExhausted,
        cpal::ErrorKind::StreamInvalidated => AudioFailureKind::StreamInvalidated,
        cpal::ErrorKind::UnsupportedConfig => AudioFailureKind::UnsupportedConfig,
        cpal::ErrorKind::UnsupportedOperation => AudioFailureKind::UnsupportedOperation,
        cpal::ErrorKind::Xrun => AudioFailureKind::Xrun,
        cpal::ErrorKind::BackendError => AudioFailureKind::BackendError,
        cpal::ErrorKind::Other => AudioFailureKind::Other,
        _ => AudioFailureKind::Other,
    }
}

fn map_sample_format(format: cpal::SampleFormat) -> AudioSampleFormat {
    match format {
        cpal::SampleFormat::F32 => AudioSampleFormat::F32,
        cpal::SampleFormat::F64 => AudioSampleFormat::F64,
        cpal::SampleFormat::I8 => AudioSampleFormat::I8,
        cpal::SampleFormat::I16 => AudioSampleFormat::I16,
        cpal::SampleFormat::I24 => AudioSampleFormat::I24,
        cpal::SampleFormat::I32 => AudioSampleFormat::I32,
        cpal::SampleFormat::I64 => AudioSampleFormat::I64,
        cpal::SampleFormat::U8 => AudioSampleFormat::U8,
        cpal::SampleFormat::U16 => AudioSampleFormat::U16,
        cpal::SampleFormat::U24 => AudioSampleFormat::U24,
        cpal::SampleFormat::U32 => AudioSampleFormat::U32,
        cpal::SampleFormat::U64 => AudioSampleFormat::U64,
        cpal::SampleFormat::DsdU8 => AudioSampleFormat::DsdU8,
        cpal::SampleFormat::DsdU16 => AudioSampleFormat::DsdU16,
        cpal::SampleFormat::DsdU32 => AudioSampleFormat::DsdU32,
        _ => AudioSampleFormat::Other(format.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_desktop_native_linux_hosts() {
        assert!(host_priority("pipewire") < host_priority("pulse audio"));
        assert!(host_priority("pulse audio") < host_priority("alsa"));
        assert!(host_priority("alsa") < host_priority("unknown"));
    }

    #[test]
    fn maps_actionable_cpal_errors() {
        assert_eq!(
            map_error_kind(cpal::ErrorKind::PermissionDenied),
            AudioFailureKind::PermissionDenied
        );
        assert_eq!(
            map_error_kind(cpal::ErrorKind::DeviceBusy),
            AudioFailureKind::DeviceBusy
        );
        assert_eq!(
            map_error_kind(cpal::ErrorKind::HostUnavailable),
            AudioFailureKind::BackendUnavailable
        );
    }

    #[test]
    fn maps_default_pcm_formats_without_loss_of_type() {
        assert_eq!(
            map_sample_format(cpal::SampleFormat::F32),
            AudioSampleFormat::F32
        );
        assert_eq!(
            map_sample_format(cpal::SampleFormat::I24),
            AudioSampleFormat::I24
        );
    }
}
