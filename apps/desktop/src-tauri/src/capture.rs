use std::error::Error;
use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use blcvoice_audio::{
    AudioDeviceId, CaptureBufferConfig, InputCaptureFactory, InputCaptureRequest,
    InputDeviceDiscovery, InputDiscovery,
};
use blcvoice_core::{SessionId, SessionSnapshot, SessionState};
use blcvoice_runtime::{DictationRuntime, FinalizationReport, RuntimeError};

const CAPTURE_PUMP_INTERVAL: Duration = Duration::from_millis(10);
pub const MICROPHONE_TEST_MAX_DURATION_MS: u32 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopCaptureErrorKind {
    Busy,
    InvalidDevice,
    StaleSession,
    PumpFailed,
    WorkerSpawn,
    WorkerJoin,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopCaptureError {
    kind: DesktopCaptureErrorKind,
    message: String,
}

impl DesktopCaptureError {
    #[must_use]
    pub fn new(kind: DesktopCaptureErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> DesktopCaptureErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for DesktopCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for DesktopCaptureError {}

impl From<RuntimeError> for DesktopCaptureError {
    fn from(error: RuntimeError) -> Self {
        Self::new(DesktopCaptureErrorKind::Runtime, error.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicrophoneTestReport {
    pub finalized: FinalizationReport,
    pub terminal_session: SessionSnapshot,
}

#[derive(Debug, Default)]
struct CaptureControl {
    start_in_flight: bool,
    worker: Option<CapturePumpWorker>,
    last_pump_failure: Option<String>,
}

pub struct DesktopCaptureService {
    discovery: Arc<dyn InputDeviceDiscovery>,
    runtime: Arc<DictationRuntime>,
    control: Mutex<CaptureControl>,
}

impl fmt::Debug for DesktopCaptureService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let control = self.lock_control();
        formatter
            .debug_struct("DesktopCaptureService")
            .field("session", &self.runtime.current())
            .field("start_in_flight", &control.start_in_flight)
            .field(
                "pump_session",
                &control.worker.as_ref().map(CapturePumpWorker::session_id),
            )
            .field("last_pump_failure", &control.last_pump_failure)
            .finish_non_exhaustive()
    }
}

impl DesktopCaptureService {
    #[must_use]
    pub fn new(
        discovery: Arc<dyn InputDeviceDiscovery>,
        capture_factory: Arc<dyn InputCaptureFactory>,
    ) -> Self {
        Self {
            discovery,
            runtime: Arc::new(DictationRuntime::new(capture_factory)),
            control: Mutex::new(CaptureControl::default()),
        }
    }

    #[must_use]
    pub fn discover_input_devices(&self) -> InputDiscovery {
        self.discovery.discover_input_devices()
    }

    #[must_use]
    pub fn current_session(&self) -> Option<SessionSnapshot> {
        self.runtime.current()
    }

    #[must_use]
    pub fn last_pump_failure(&self) -> Option<String> {
        self.lock_control().last_pump_failure.clone()
    }

    pub fn start_microphone_test(
        &self,
        device_id: AudioDeviceId,
    ) -> Result<SessionSnapshot, DesktopCaptureError> {
        {
            let mut control = self.lock_control();
            Self::reap_finished_worker(&mut control);
            if control.start_in_flight || control.worker.is_some() {
                return Err(DesktopCaptureError::new(
                    DesktopCaptureErrorKind::Busy,
                    "a desktop microphone capture operation is already active",
                ));
            }
            control.start_in_flight = true;
            control.last_pump_failure = None;
        }

        let request = InputCaptureRequest {
            device_id,
            buffer: CaptureBufferConfig::default(),
        };
        let start_result = self
            .runtime
            .start_recording(&request, MICROPHONE_TEST_MAX_DURATION_MS);

        let mut control = self.lock_control();
        control.start_in_flight = false;
        let session = start_result.map_err(DesktopCaptureError::from)?;

        let worker = match CapturePumpWorker::spawn(Arc::clone(&self.runtime), session.id) {
            Ok(worker) => worker,
            Err(error) => {
                drop(control);
                let _ = self.runtime.cancel(session.id);
                return Err(error);
            }
        };
        control.worker = Some(worker);

        Ok(session)
    }

    pub fn finish_microphone_test(
        &self,
        session_id: SessionId,
    ) -> Result<MicrophoneTestReport, DesktopCaptureError> {
        let worker = self.take_worker(session_id)?;
        match worker.stop_and_join()? {
            PumpExit::Stopped => {}
            PumpExit::Failed(message) => {
                self.lock_control().last_pump_failure = Some(message.clone());
                return Err(DesktopCaptureError::new(
                    DesktopCaptureErrorKind::PumpFailed,
                    message,
                ));
            }
        }

        let finalized = self.runtime.finalize_recording(session_id)?;
        let terminal_session = self.runtime.cancel(session_id)?;

        Ok(MicrophoneTestReport {
            finalized,
            terminal_session,
        })
    }

    pub fn cancel_microphone_test(
        &self,
        session_id: SessionId,
    ) -> Result<SessionSnapshot, DesktopCaptureError> {
        let worker = {
            let mut control = self.lock_control();
            if control.start_in_flight {
                return Err(DesktopCaptureError::new(
                    DesktopCaptureErrorKind::Busy,
                    "microphone capture startup is still in progress",
                ));
            }
            match control.worker.as_ref() {
                Some(worker) if worker.session_id() != session_id => {
                    return Err(stale_worker(session_id, worker.session_id()));
                }
                Some(_) => control.worker.take(),
                None => None,
            }
        };

        if let Some(worker) = worker
            && let PumpExit::Failed(message) = worker.stop_and_join()?
        {
            self.lock_control().last_pump_failure = Some(message.clone());
            return Err(DesktopCaptureError::new(
                DesktopCaptureErrorKind::PumpFailed,
                message,
            ));
        }

        self.runtime
            .cancel(session_id)
            .map_err(DesktopCaptureError::from)
    }

    fn take_worker(&self, session_id: SessionId) -> Result<CapturePumpWorker, DesktopCaptureError> {
        let mut control = self.lock_control();
        let Some(worker) = control.worker.as_ref() else {
            return Err(DesktopCaptureError::new(
                DesktopCaptureErrorKind::Busy,
                "there is no active desktop capture worker to finish",
            ));
        };
        if worker.session_id() != session_id {
            return Err(stale_worker(session_id, worker.session_id()));
        }
        control.worker.take().ok_or_else(|| {
            DesktopCaptureError::new(
                DesktopCaptureErrorKind::WorkerJoin,
                "desktop capture worker disappeared while being claimed",
            )
        })
    }

    fn reap_finished_worker(control: &mut CaptureControl) {
        if !control
            .worker
            .as_ref()
            .is_some_and(CapturePumpWorker::is_finished)
        {
            return;
        }

        let Some(worker) = control.worker.take() else {
            return;
        };
        match worker.join_without_signal() {
            Ok(PumpExit::Stopped) => {}
            Ok(PumpExit::Failed(message)) => control.last_pump_failure = Some(message),
            Err(error) => control.last_pump_failure = Some(error.to_string()),
        }
    }

    fn lock_control(&self) -> MutexGuard<'_, CaptureControl> {
        self.control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn stale_worker(supplied: SessionId, active: SessionId) -> DesktopCaptureError {
    DesktopCaptureError::new(
        DesktopCaptureErrorKind::StaleSession,
        format!(
            "microphone test session {} is stale; capture worker belongs to session {}",
            supplied.get(),
            active.get()
        ),
    )
}

#[derive(Debug)]
struct CapturePumpWorker {
    session_id: SessionId,
    stop: Sender<()>,
    handle: JoinHandle<PumpExit>,
}

impl CapturePumpWorker {
    fn spawn(
        runtime: Arc<DictationRuntime>,
        session_id: SessionId,
    ) -> Result<Self, DesktopCaptureError> {
        let (stop, receiver) = mpsc::channel();
        let handle = thread::Builder::new()
            .name(format!("blcvoice-capture-pump-{}", session_id.get()))
            .spawn(move || pump_capture(runtime, session_id, receiver))
            .map_err(|error| {
                DesktopCaptureError::new(
                    DesktopCaptureErrorKind::WorkerSpawn,
                    format!("could not start desktop capture pump worker: {error}"),
                )
            })?;

        Ok(Self {
            session_id,
            stop,
            handle,
        })
    }

    const fn session_id(&self) -> SessionId {
        self.session_id
    }

    fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    fn stop_and_join(self) -> Result<PumpExit, DesktopCaptureError> {
        let _ = self.stop.send(());
        self.join_handle()
    }

    fn join_without_signal(self) -> Result<PumpExit, DesktopCaptureError> {
        self.join_handle()
    }

    fn join_handle(self) -> Result<PumpExit, DesktopCaptureError> {
        self.handle.join().map_err(|_| {
            DesktopCaptureError::new(
                DesktopCaptureErrorKind::WorkerJoin,
                "desktop capture pump worker panicked",
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PumpExit {
    Stopped,
    Failed(String),
}

fn pump_capture(
    runtime: Arc<DictationRuntime>,
    session_id: SessionId,
    stop: Receiver<()>,
) -> PumpExit {
    loop {
        if let Err(error) = runtime.pump_recording(session_id) {
            return PumpExit::Failed(error.to_string());
        }

        match stop.recv_timeout(CAPTURE_PUMP_INTERVAL) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => return PumpExit::Stopped,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[must_use]
pub const fn session_state_name(state: SessionState) -> &'static str {
    match state {
        SessionState::Arming => "arming",
        SessionState::Recording => "recording",
        SessionState::FinalizingAudio => "finalizingAudio",
        SessionState::Transcribing => "transcribing",
        SessionState::Transforming => "transforming",
        SessionState::Inserting => "inserting",
        SessionState::Completed => "completed",
        SessionState::Failed => "failed",
        SessionState::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blcvoice_audio::{
        AudioBackend, AudioFailure, AudioSampleFormat, AudioStreamConfig, CaptureStats,
        InputCaptureSession, InputDeviceInfo,
    };

    #[derive(Debug)]
    struct FakeDiscovery;

    impl InputDeviceDiscovery for FakeDiscovery {
        fn discover_input_devices(&self) -> InputDiscovery {
            InputDiscovery {
                selected_backend: Some(AudioBackend::Other("fake".to_owned())),
                devices: vec![InputDeviceInfo {
                    id: fake_device_id(),
                    name: "Fake microphone".to_owned(),
                    backend: AudioBackend::Other("fake".to_owned()),
                    is_default: true,
                    default_config: Some(test_stream_config()),
                }],
                failures: Vec::new(),
            }
        }
    }

    #[derive(Debug, Clone)]
    struct FakeCaptureFactory {
        samples: Vec<f32>,
    }

    impl InputCaptureFactory for FakeCaptureFactory {
        fn start_input_capture(
            &self,
            _request: &InputCaptureRequest,
        ) -> Result<Box<dyn InputCaptureSession>, AudioFailure> {
            Ok(Box::new(FakeCapture {
                config: test_stream_config(),
                samples: self.samples.clone(),
                position: 0,
            }))
        }
    }

    #[derive(Debug)]
    struct InvalidReadCaptureFactory;

    impl InputCaptureFactory for InvalidReadCaptureFactory {
        fn start_input_capture(
            &self,
            _request: &InputCaptureRequest,
        ) -> Result<Box<dyn InputCaptureSession>, AudioFailure> {
            Ok(Box::new(InvalidReadCapture {
                config: test_stream_config(),
            }))
        }
    }

    #[derive(Debug)]
    struct InvalidReadCapture {
        config: AudioStreamConfig,
    }

    impl InputCaptureSession for InvalidReadCapture {
        fn stream_config(&self) -> &AudioStreamConfig {
            &self.config
        }

        fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
            output.len().saturating_add(1)
        }

        fn stats(&self) -> CaptureStats {
            CaptureStats::default()
        }

        fn pause(&self) -> Result<(), AudioFailure> {
            Ok(())
        }

        fn resume(&self) -> Result<(), AudioFailure> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FakeCapture {
        config: AudioStreamConfig,
        samples: Vec<f32>,
        position: usize,
    }

    impl InputCaptureSession for FakeCapture {
        fn stream_config(&self) -> &AudioStreamConfig {
            &self.config
        }

        fn read_interleaved_f32(&mut self, output: &mut [f32]) -> usize {
            let remaining = &self.samples[self.position..];
            if remaining.is_empty() {
                return 0;
            }
            let copied = remaining.len().min(output.len());
            output[..copied].copy_from_slice(&remaining[..copied]);
            self.position += copied;
            copied
        }

        fn stats(&self) -> CaptureStats {
            CaptureStats {
                received_samples: self.samples.len() as u64,
                ..CaptureStats::default()
            }
        }

        fn pause(&self) -> Result<(), AudioFailure> {
            Ok(())
        }

        fn resume(&self) -> Result<(), AudioFailure> {
            Ok(())
        }
    }

    fn service() -> DesktopCaptureService {
        DesktopCaptureService::new(
            Arc::new(FakeDiscovery),
            Arc::new(FakeCaptureFactory {
                samples: vec![0.1, 0.2, 0.3, 0.4],
            }),
        )
    }

    fn invalid_read_service() -> DesktopCaptureService {
        DesktopCaptureService::new(Arc::new(FakeDiscovery), Arc::new(InvalidReadCaptureFactory))
    }

    fn fake_device_id() -> AudioDeviceId {
        AudioDeviceId::new("fake:microphone").expect("fake device id must be valid")
    }

    fn test_stream_config() -> AudioStreamConfig {
        AudioStreamConfig {
            channels: 1,
            sample_rate_hz: 16_000,
            sample_format: AudioSampleFormat::F32,
        }
    }

    #[test]
    fn discovers_devices_through_runtime_independent_contract() {
        let service = service();
        let discovery = service.discover_input_devices();

        assert!(discovery.has_usable_input());
        assert_eq!(discovery.devices.len(), 1);
        assert_eq!(discovery.devices[0].id, fake_device_id());
    }

    #[test]
    fn microphone_test_finalizes_audio_then_discards_it_by_cancelling() {
        let service = service();
        let session = service
            .start_microphone_test(fake_device_id())
            .expect("microphone test must start");

        let report = service
            .finish_microphone_test(session.id)
            .expect("microphone test must finish");

        assert_eq!(session.state, SessionState::Recording);
        assert_eq!(report.finalized.source_frames, 4);
        assert_eq!(report.finalized.session.state, SessionState::Transcribing);
        assert_eq!(report.terminal_session.state, SessionState::Cancelled);
        assert_eq!(service.current_session(), Some(report.terminal_session));
    }

    #[test]
    fn second_test_is_rejected_while_capture_worker_is_active() {
        let service = service();
        let first = service
            .start_microphone_test(fake_device_id())
            .expect("first microphone test must start");

        let error = service
            .start_microphone_test(fake_device_id())
            .expect_err("overlapping microphone tests must be rejected");

        assert_eq!(error.kind(), DesktopCaptureErrorKind::Busy);
        service
            .cancel_microphone_test(first.id)
            .expect("first microphone test must cancel");
    }

    #[test]
    fn stale_session_cannot_claim_another_sessions_worker() {
        let service = service();
        let active = service
            .start_microphone_test(fake_device_id())
            .expect("microphone test must start");
        let stale = SessionId::new(active.id.get() + 99);

        let error = service
            .finish_microphone_test(stale)
            .expect_err("stale session must not take active worker");

        assert_eq!(error.kind(), DesktopCaptureErrorKind::StaleSession);
        service
            .cancel_microphone_test(active.id)
            .expect("active test must remain cancellable");
    }

    #[test]
    fn cancel_is_rejected_while_capture_start_is_in_flight() {
        let service = service();
        service.lock_control().start_in_flight = true;

        let error = service
            .cancel_microphone_test(SessionId::new(1))
            .expect_err("cancel must not race capture startup");

        assert_eq!(error.kind(), DesktopCaptureErrorKind::Busy);
    }

    #[test]
    fn finished_failed_worker_is_reported_as_pump_failure() {
        let service = invalid_read_service();
        let session = service
            .start_microphone_test(fake_device_id())
            .expect("microphone test must start");
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            let finished = service
                .lock_control()
                .worker
                .as_ref()
                .is_some_and(CapturePumpWorker::is_finished);
            if finished {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "capture pump worker did not finish after invalid read"
            );
            thread::yield_now();
        }

        let error = service
            .finish_microphone_test(session.id)
            .expect_err("finished failed worker must preserve its pump failure");

        assert_eq!(error.kind(), DesktopCaptureErrorKind::PumpFailed);
        assert_eq!(
            service.current_session().map(|snapshot| snapshot.state),
            Some(SessionState::Failed)
        );
    }

    #[test]
    fn terminal_session_can_be_replaced_by_a_new_microphone_test() {
        let service = service();
        let first = service
            .start_microphone_test(fake_device_id())
            .expect("first microphone test must start");
        service
            .cancel_microphone_test(first.id)
            .expect("first microphone test must cancel");

        let second = service
            .start_microphone_test(fake_device_id())
            .expect("second microphone test must start");

        assert_eq!(second.id.get(), first.id.get() + 1);
        service
            .cancel_microphone_test(second.id)
            .expect("second microphone test must cancel");
    }
}
