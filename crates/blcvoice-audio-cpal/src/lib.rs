#![forbid(unsafe_code)]

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use blcvoice_audio::{
    AudioBackend, AudioDeviceId, AudioFailure, AudioFailureKind, AudioSampleFormat,
    AudioStreamConfig, CaptureStats, InputCaptureFactory, InputCaptureRequest, InputCaptureSession,
    InputDeviceDiscovery, InputDeviceInfo, InputDiscovery,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};

const STREAM_INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Starts CPAL input streams while exposing only the runtime-independent capture contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpalInputCaptureFactory;

impl InputCaptureFactory for CpalInputCaptureFactory {
    fn start_input_capture(
        &self,
        request: &InputCaptureRequest,
    ) -> Result<Box<dyn InputCaptureSession>, AudioFailure> {
        let cpal_device_id =
            cpal::DeviceId::from_str(request.device_id.as_str()).map_err(|error| AudioFailure {
                backend: None,
                device_id: Some(request.device_id.clone()),
                kind: AudioFailureKind::InvalidInput,
                message: format!("invalid audio device id: {error}"),
            })?;
        let backend = AudioBackend::from_host_name(&cpal_device_id.host().to_string());
        let host = cpal::host_from_id(cpal_device_id.host()).map_err(|error| {
            failure(
                Some(backend.clone()),
                Some(request.device_id.clone()),
                &error,
            )
        })?;
        let device = host
            .device_by_id(&cpal_device_id)
            .ok_or_else(|| AudioFailure {
                backend: Some(backend.clone()),
                device_id: Some(request.device_id.clone()),
                kind: AudioFailureKind::DeviceNotAvailable,
                message: "selected input device is no longer available".to_owned(),
            })?;

        let supported = device.default_input_config().map_err(|error| {
            failure(
                Some(backend.clone()),
                Some(request.device_id.clone()),
                &error,
            )
        })?;
        let sample_format = supported.sample_format();
        if !is_pcm_sample_format(sample_format) {
            return Err(AudioFailure {
                backend: Some(backend),
                device_id: Some(request.device_id.clone()),
                kind: AudioFailureKind::UnsupportedConfig,
                message: format!(
                    "input sample format {sample_format} is not supported by the dictation capture path"
                ),
            });
        }

        let stream_config = AudioStreamConfig {
            channels: supported.channels(),
            sample_rate_hz: supported.sample_rate(),
            sample_format: map_sample_format(sample_format),
        };
        let capacity = request
            .buffer
            .capacity_samples(&stream_config)
            .map_err(|error| AudioFailure {
                backend: Some(backend.clone()),
                device_id: Some(request.device_id.clone()),
                kind: AudioFailureKind::InvalidInput,
                message: error.to_string(),
            })?;

        let (mut producer, consumer) = RingBuffer::<f32>::new(capacity);
        let metrics = Arc::new(CaptureMetrics::default());
        let data_metrics = Arc::clone(&metrics);
        let error_metrics = Arc::clone(&metrics);
        let cpal_stream_config = supported.config();

        let stream = device
            .build_input_stream_raw(
                cpal_stream_config,
                sample_format,
                move |data, _info| ingest_data(data, &mut producer, &data_metrics),
                move |error| error_metrics.record_error(map_error_kind(error.kind())),
                Some(STREAM_INITIALIZATION_TIMEOUT),
            )
            .map_err(|error| {
                failure(
                    Some(backend.clone()),
                    Some(request.device_id.clone()),
                    &error,
                )
            })?;

        stream.play().map_err(|error| {
            failure(
                Some(backend.clone()),
                Some(request.device_id.clone()),
                &error,
            )
        })?;

        Ok(Box::new(CpalInputCapture {
            stream,
            consumer,
            metrics,
            stream_config,
            backend,
            device_id: request.device_id.clone(),
        }))
    }
}

struct CpalInputCapture {
    stream: cpal::Stream,
    consumer: Consumer<f32>,
    metrics: Arc<CaptureMetrics>,
    stream_config: AudioStreamConfig,
    backend: AudioBackend,
    device_id: AudioDeviceId,
}

impl InputCaptureSession for CpalInputCapture {
    fn stream_config(&self) -> &AudioStreamConfig {
        &self.stream_config
    }

    fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
        let (filled, _unused) = self.consumer.pop_partial_slice(output);
        filled.len()
    }

    fn stats(&self) -> CaptureStats {
        self.metrics.snapshot()
    }

    fn pause(&self) -> Result<(), AudioFailure> {
        self.stream.pause().map_err(|error| {
            failure(
                Some(self.backend.clone()),
                Some(self.device_id.clone()),
                &error,
            )
        })
    }

    fn resume(&self) -> Result<(), AudioFailure> {
        self.stream.play().map_err(|error| {
            failure(
                Some(self.backend.clone()),
                Some(self.device_id.clone()),
                &error,
            )
        })
    }
}

#[derive(Debug, Default)]
struct CaptureMetrics {
    received_samples: AtomicU64,
    dropped_samples: AtomicU64,
    callback_errors: AtomicU64,
    last_failure: AtomicU8,
}

impl CaptureMetrics {
    fn record_samples(&self, received: usize, dropped: usize) {
        self.received_samples
            .fetch_add(received as u64, Ordering::Relaxed);
        self.dropped_samples
            .fetch_add(dropped as u64, Ordering::Relaxed);
    }

    fn record_error(&self, kind: AudioFailureKind) {
        self.callback_errors.fetch_add(1, Ordering::Relaxed);
        self.last_failure
            .store(encode_failure_kind(kind), Ordering::Relaxed);
    }

    fn snapshot(&self) -> CaptureStats {
        CaptureStats {
            received_samples: self.received_samples.load(Ordering::Relaxed),
            dropped_samples: self.dropped_samples.load(Ordering::Relaxed),
            callback_errors: self.callback_errors.load(Ordering::Relaxed),
            last_failure: decode_failure_kind(self.last_failure.load(Ordering::Relaxed)),
        }
    }
}

fn ingest_data(data: &cpal::Data, producer: &mut Producer<f32>, metrics: &CaptureMetrics) {
    match data.sample_format() {
        cpal::SampleFormat::F32 => ingest_typed(data.as_slice::<f32>(), producer, metrics),
        cpal::SampleFormat::F64 => ingest_typed(data.as_slice::<f64>(), producer, metrics),
        cpal::SampleFormat::I8 => ingest_typed(data.as_slice::<i8>(), producer, metrics),
        cpal::SampleFormat::I16 => ingest_typed(data.as_slice::<i16>(), producer, metrics),
        cpal::SampleFormat::I24 => ingest_typed(data.as_slice::<cpal::I24>(), producer, metrics),
        cpal::SampleFormat::I32 => ingest_typed(data.as_slice::<i32>(), producer, metrics),
        cpal::SampleFormat::I64 => ingest_typed(data.as_slice::<i64>(), producer, metrics),
        cpal::SampleFormat::U8 => ingest_typed(data.as_slice::<u8>(), producer, metrics),
        cpal::SampleFormat::U16 => ingest_typed(data.as_slice::<u16>(), producer, metrics),
        cpal::SampleFormat::U24 => ingest_typed(data.as_slice::<cpal::U24>(), producer, metrics),
        cpal::SampleFormat::U32 => ingest_typed(data.as_slice::<u32>(), producer, metrics),
        cpal::SampleFormat::U64 => ingest_typed(data.as_slice::<u64>(), producer, metrics),
        _ => {
            metrics.record_samples(data.len(), data.len());
            metrics.record_error(AudioFailureKind::UnsupportedConfig);
        }
    }
}

fn ingest_typed<T>(samples: Option<&[T]>, producer: &mut Producer<f32>, metrics: &CaptureMetrics)
where
    T: Copy,
    f32: cpal::FromSample<T>,
{
    let Some(samples) = samples else {
        metrics.record_error(AudioFailureKind::BackendError);
        return;
    };

    let writable = producer.slots().min(samples.len());
    if writable == 0 {
        metrics.record_samples(samples.len(), samples.len());
        return;
    }

    let Ok(mut chunk) = producer.write_chunk(writable) else {
        metrics.record_samples(samples.len(), samples.len());
        metrics.record_error(AudioFailureKind::BackendError);
        return;
    };

    let (first, second) = chunk.as_mut_slices();
    let first_len = first.len();

    for (destination, source) in first.iter_mut().zip(samples.iter()) {
        *destination = <f32 as cpal::FromSample<T>>::from_sample_(*source);
    }
    for (destination, source) in second.iter_mut().zip(samples[first_len..].iter()) {
        *destination = <f32 as cpal::FromSample<T>>::from_sample_(*source);
    }

    chunk.commit_all();
    metrics.record_samples(samples.len(), samples.len() - writable);
}

fn is_pcm_sample_format(format: cpal::SampleFormat) -> bool {
    !matches!(
        format,
        cpal::SampleFormat::DsdU8 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU32
    )
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

fn failure(
    backend: Option<AudioBackend>,
    device_id: Option<AudioDeviceId>,
    error: &cpal::Error,
) -> AudioFailure {
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

fn encode_failure_kind(kind: AudioFailureKind) -> u8 {
    match kind {
        AudioFailureKind::NoInputDevices => 1,
        AudioFailureKind::DeviceBusy => 2,
        AudioFailureKind::DeviceChanged => 3,
        AudioFailureKind::DeviceNotAvailable => 4,
        AudioFailureKind::BackendUnavailable => 5,
        AudioFailureKind::InvalidInput => 6,
        AudioFailureKind::PermissionDenied => 7,
        AudioFailureKind::RealtimeDenied => 8,
        AudioFailureKind::ResourceExhausted => 9,
        AudioFailureKind::StreamInvalidated => 10,
        AudioFailureKind::UnsupportedConfig => 11,
        AudioFailureKind::UnsupportedOperation => 12,
        AudioFailureKind::Xrun => 13,
        AudioFailureKind::BackendError => 14,
        AudioFailureKind::Other => 15,
    }
}

fn decode_failure_kind(value: u8) -> Option<AudioFailureKind> {
    match value {
        1 => Some(AudioFailureKind::NoInputDevices),
        2 => Some(AudioFailureKind::DeviceBusy),
        3 => Some(AudioFailureKind::DeviceChanged),
        4 => Some(AudioFailureKind::DeviceNotAvailable),
        5 => Some(AudioFailureKind::BackendUnavailable),
        6 => Some(AudioFailureKind::InvalidInput),
        7 => Some(AudioFailureKind::PermissionDenied),
        8 => Some(AudioFailureKind::RealtimeDenied),
        9 => Some(AudioFailureKind::ResourceExhausted),
        10 => Some(AudioFailureKind::StreamInvalidated),
        11 => Some(AudioFailureKind::UnsupportedConfig),
        12 => Some(AudioFailureKind::UnsupportedOperation),
        13 => Some(AudioFailureKind::Xrun),
        14 => Some(AudioFailureKind::BackendError),
        15 => Some(AudioFailureKind::Other),
        _ => None,
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

    #[test]
    fn realtime_handoff_drops_only_the_overflow_without_blocking() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(3);
        let metrics = CaptureMetrics::default();

        ingest_typed(
            Some(&[1.0_f32, 2.0, 3.0, 4.0, 5.0]),
            &mut producer,
            &metrics,
        );

        let mut output = [0.0_f32; 5];
        let read = {
            let (filled, _unused) = consumer.pop_partial_slice(&mut output);
            filled.len()
        };

        assert_eq!(read, 3);
        assert_eq!(&output[..read], &[1.0, 2.0, 3.0]);
        assert_eq!(
            metrics.snapshot(),
            CaptureStats {
                received_samples: 5,
                dropped_samples: 2,
                callback_errors: 0,
                last_failure: None,
            }
        );
    }

    #[test]
    fn callback_failures_are_recorded_without_a_mutex() {
        let metrics = CaptureMetrics::default();
        metrics.record_error(AudioFailureKind::DeviceChanged);

        assert_eq!(metrics.snapshot().callback_errors, 1);
        assert_eq!(
            metrics.snapshot().last_failure,
            Some(AudioFailureKind::DeviceChanged)
        );
    }

    #[test]
    fn dsd_is_rejected_from_the_pcm_dictation_path() {
        assert!(!is_pcm_sample_format(cpal::SampleFormat::DsdU8));
        assert!(is_pcm_sample_format(cpal::SampleFormat::F32));
    }
}
