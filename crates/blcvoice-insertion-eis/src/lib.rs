#![forbid(unsafe_code)]

use blcvoice_insertion::{
    InsertionAuthorization, InsertionBackend, InsertionCapability, InsertionError,
    InsertionErrorKind, InsertionReceipt, TextInserter,
};

#[cfg(any(target_os = "linux", test))]
const EI_TEXT_MAX_UTF8_BYTES: usize = 254;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaylandEisOptions {
    restore_token: Option<String>,
}

impl WaylandEisOptions {
    #[must_use]
    pub fn new(restore_token: Option<String>) -> Self {
        Self { restore_token }
    }

    #[must_use]
    pub fn restore_token(&self) -> Option<&str> {
        self.restore_token.as_deref()
    }
}

const fn wayland_eis_capability() -> InsertionCapability {
    InsertionCapability::new(
        InsertionBackend::XdgRemoteDesktopEis,
        InsertionAuthorization::XdgRemoteDesktop,
    )
}

fn validate_text(text: &str) -> Result<(), InsertionError> {
    if text.is_empty() {
        return Err(InsertionError::new(
            InsertionErrorKind::InvalidText,
            "cannot submit empty text through ei_text.utf8",
        ));
    }
    if text.contains('\0') {
        return Err(InsertionError::new(
            InsertionErrorKind::InvalidText,
            "text contains a NUL byte, which ei_text.utf8 cannot encode",
        ));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn utf8_chunks(mut text: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        if text.is_empty() {
            return None;
        }

        let mut end = EI_TEXT_MAX_UTF8_BYTES.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let (chunk, rest) = text.split_at(end);
        text = rest;
        Some(chunk)
    })
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{
        os::unix::net::UnixStream,
        sync::mpsc,
        thread::{self, JoinHandle},
        time::Duration,
    };

    use ashpd::desktop::{
        CreateSessionOptions, PersistMode,
        remote_desktop::{
            ConnectToEISOptions, DeviceType as PortalDeviceType, RemoteDesktop,
            SelectDevicesOptions, StartOptions,
        },
    };
    use futures_util::StreamExt;
    use reis::{
        ei,
        event::{DeviceCapability, EiEvent},
    };
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    use super::{
        InsertionBackend, InsertionError, InsertionErrorKind, InsertionReceipt, TextInserter,
        WaylandEisOptions, utf8_chunks, validate_text, wayland_eis_capability,
    };

    const DEVICE_READY_TIMEOUT: Duration = Duration::from_secs(5);

    enum WorkerCommand {
        Insert {
            text: String,
            reply: mpsc::SyncSender<Result<InsertionReceipt, InsertionError>>,
        },
        Shutdown,
    }

    struct DeviceSlot {
        device: reis::event::Device,
        resumed: bool,
        emulating: bool,
    }

    struct EiState {
        devices: Vec<DeviceSlot>,
        last_serial: u32,
        sequence: u32,
    }

    impl EiState {
        fn new() -> Self {
            Self {
                devices: Vec::new(),
                last_serial: 0,
                sequence: 0,
            }
        }

        fn has_resumed_text_device(&self) -> bool {
            self.devices
                .iter()
                .any(|slot| slot.resumed && slot.device.has_capability(DeviceCapability::Text))
        }

        fn handle_event(
            &mut self,
            event: EiEvent,
            context: &ei::Context,
        ) -> Result<(), InsertionError> {
            match event {
                EiEvent::Disconnected(_) => {
                    return Err(InsertionError::new(
                        InsertionErrorKind::BackendUnavailable,
                        "the EIS implementation disconnected BLCVoice",
                    ));
                }
                EiEvent::SeatAdded(event) => {
                    event
                        .seat
                        .bind_capabilities(DeviceCapability::Keyboard | DeviceCapability::Text);
                    context.flush().map_err(|error| {
                        backend_failure(format!("failed to bind EIS text capabilities: {error}"))
                    })?;
                }
                EiEvent::DeviceAdded(event) => {
                    if event.device.device().version() >= 3 {
                        event.device.device().ready();
                        context.flush().map_err(|error| {
                            backend_failure(format!("failed to mark EIS device ready: {error}"))
                        })?;
                    }
                    self.devices.push(DeviceSlot {
                        device: event.device,
                        resumed: false,
                        emulating: false,
                    });
                }
                EiEvent::DeviceRemoved(event) => {
                    self.devices.retain(|slot| slot.device != event.device);
                }
                EiEvent::DeviceResumed(event) => {
                    self.last_serial = event.serial;
                    if let Some(slot) = self
                        .devices
                        .iter_mut()
                        .find(|slot| slot.device == event.device)
                    {
                        slot.resumed = true;
                        slot.emulating = false;
                    }
                }
                EiEvent::DevicePaused(event) => {
                    self.last_serial = event.serial;
                    if let Some(slot) = self
                        .devices
                        .iter_mut()
                        .find(|slot| slot.device == event.device)
                    {
                        slot.resumed = false;
                        slot.emulating = false;
                    }
                }
                EiEvent::KeyboardModifiers(event) => {
                    self.last_serial = event.serial;
                }
                _ => {}
            }
            Ok(())
        }

        fn text_device_index(&self) -> Option<usize> {
            self.devices
                .iter()
                .position(|slot| slot.resumed && slot.device.has_capability(DeviceCapability::Text))
        }

        fn ensure_emulating(&mut self, index: usize, device: &reis::event::Device) {
            if self.devices[index].emulating {
                return;
            }
            device
                .device()
                .start_emulating(self.last_serial, self.sequence);
            self.sequence = self.sequence.wrapping_add(1);
            self.devices[index].emulating = true;
        }

        fn insert(
            &mut self,
            context: &ei::Context,
            text: &str,
        ) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;

            let index = self.text_device_index().ok_or_else(|| {
                InsertionError::new(
                    InsertionErrorKind::BackendUnavailable,
                    "no resumed EIS device exposes the ei_text interface",
                )
            })?;
            let device = self.devices[index].device.clone();
            self.ensure_emulating(index, &device);

            let text_interface = device.interface::<ei::Text>().ok_or_else(|| {
                InsertionError::new(
                    InsertionErrorKind::BackendUnavailable,
                    "the selected EIS device no longer exposes ei_text",
                )
            })?;

            let mut submitted = 0usize;
            for chunk in utf8_chunks(text) {
                text_interface.utf8(chunk);
                device
                    .device()
                    .frame(self.last_serial, monotonic_microseconds());

                if let Err(error) = context.flush() {
                    let kind = if submitted == 0 {
                        InsertionErrorKind::BackendFailure
                    } else {
                        InsertionErrorKind::PartialSubmission
                    };
                    return Err(InsertionError::new(
                        kind,
                        format!(
                            "EIS flush failed after {submitted} UTF-8 bytes were accepted: {error}"
                        ),
                    ));
                }
                submitted += chunk.len();
            }

            Ok(InsertionReceipt::complete(
                InsertionBackend::XdgRemoteDesktopEis,
                submitted,
            ))
        }
    }

    pub struct WaylandEisInserter {
        command_tx: UnboundedSender<WorkerCommand>,
        restore_token: Option<String>,
        worker: Option<JoinHandle<()>>,
    }

    impl WaylandEisInserter {
        pub fn connect(options: WaylandEisOptions) -> Result<Self, InsertionError> {
            let (command_tx, command_rx) = unbounded_channel();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);

            let worker = thread::Builder::new()
                .name("blcvoice-wayland-eis".to_owned())
                .spawn(move || worker_main(options, command_rx, ready_tx))
                .map_err(|error| {
                    backend_failure(format!("failed to start the EIS worker thread: {error}"))
                })?;

            match ready_rx.recv() {
                Ok(Ok(restore_token)) => Ok(Self {
                    command_tx,
                    restore_token,
                    worker: Some(worker),
                }),
                Ok(Err(error)) => {
                    let _ = worker.join();
                    Err(error)
                }
                Err(error) => {
                    let _ = worker.join();
                    Err(backend_failure(format!(
                        "EIS worker exited before initialization completed: {error}"
                    )))
                }
            }
        }

        #[must_use]
        pub fn restore_token(&self) -> Option<&str> {
            self.restore_token.as_deref()
        }
    }

    impl TextInserter for WaylandEisInserter {
        fn capability(&self) -> super::InsertionCapability {
            wayland_eis_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;

            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            self.command_tx
                .send(WorkerCommand::Insert {
                    text: text.to_owned(),
                    reply: reply_tx,
                })
                .map_err(|_| {
                    InsertionError::new(
                        InsertionErrorKind::BackendUnavailable,
                        "the Wayland EIS worker is no longer running",
                    )
                })?;

            reply_rx.recv().map_err(|error| {
                backend_failure(format!(
                    "the Wayland EIS worker exited before reporting insertion status: {error}"
                ))
            })?
        }
    }

    impl Drop for WaylandEisInserter {
        fn drop(&mut self) {
            let _ = self.command_tx.send(WorkerCommand::Shutdown);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn worker_main(
        options: WaylandEisOptions,
        command_rx: UnboundedReceiver<WorkerCommand>,
        ready_tx: mpsc::SyncSender<Result<Option<String>, InsertionError>>,
    ) {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready_tx.send(Err(backend_failure(format!(
                    "failed to initialize the EIS async runtime: {error}"
                ))));
                return;
            }
        };

        runtime.block_on(async move {
            let result = run_worker(options, command_rx, ready_tx.clone()).await;
            if let Err(error) = result {
                let _ = ready_tx.try_send(Err(error));
            }
        });
    }

    async fn run_worker(
        options: WaylandEisOptions,
        mut command_rx: UnboundedReceiver<WorkerCommand>,
        ready_tx: mpsc::SyncSender<Result<Option<String>, InsertionError>>,
    ) -> Result<(), InsertionError> {
        let remote_desktop = RemoteDesktop::new().await.map_err(|error| {
            backend_failure(format!(
                "failed to connect to the RemoteDesktop portal: {error}"
            ))
        })?;
        let session = remote_desktop
            .create_session(CreateSessionOptions::default())
            .await
            .map_err(|error| {
                backend_failure(format!("failed to create a RemoteDesktop session: {error}"))
            })?;

        let mut select_options = SelectDevicesOptions::default()
            .set_devices(enumflags2::BitFlags::from_flag(PortalDeviceType::Keyboard))
            .set_persist_mode(PersistMode::ExplicitlyRevoked);
        if let Some(token) = options.restore_token() {
            select_options = select_options.set_restore_token(token);
        }

        remote_desktop
            .select_devices(&session, select_options)
            .await
            .map_err(|error| {
                backend_failure(format!(
                    "failed to request portal keyboard control: {error}"
                ))
            })?;

        let start_request = remote_desktop
            .start(&session, None, StartOptions::default())
            .await
            .map_err(|error| {
                backend_failure(format!(
                    "failed to start the RemoteDesktop session: {error}"
                ))
            })?;
        let start_response = start_request.response().map_err(|error| {
            InsertionError::new(
                InsertionErrorKind::PermissionDenied,
                format!("RemoteDesktop keyboard authorization was denied or cancelled: {error}"),
            )
        })?;
        let restore_token = start_response.restore_token().map(str::to_owned);

        let fd = remote_desktop
            .connect_to_eis(&session, ConnectToEISOptions::default())
            .await
            .map_err(|error| {
                backend_failure(format!("failed to obtain the portal EIS socket: {error}"))
            })?;
        let stream = UnixStream::from(fd);
        stream.set_nonblocking(true).map_err(|error| {
            backend_failure(format!("failed to configure the EIS socket: {error}"))
        })?;

        let context = ei::Context::new(stream)
            .map_err(|error| backend_failure(format!("failed to create EIS context: {error}")))?;
        let (_connection, mut events) = context
            .handshake_tokio("BLCVoice", ei::handshake::ContextType::Sender)
            .await
            .map_err(|error| backend_failure(format!("EIS sender handshake failed: {error}")))?;

        let mut state = EiState::new();
        tokio::time::timeout(DEVICE_READY_TIMEOUT, async {
            while !state.has_resumed_text_device() {
                let event = events
                    .next()
                    .await
                    .ok_or_else(|| {
                        InsertionError::new(
                            InsertionErrorKind::BackendUnavailable,
                            "the EIS event stream closed before a text device became available",
                        )
                    })?
                    .map_err(|error| {
                        backend_failure(format!("failed to read an EIS readiness event: {error}"))
                    })?;
                state.handle_event(event, &context)?;
            }
            Ok::<(), InsertionError>(())
        })
        .await
        .map_err(|_| {
            InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the compositor did not expose a resumed ei_text device within five seconds",
            )
        })??;

        ready_tx.send(Ok(restore_token)).map_err(|_| {
            backend_failure("the EIS owner disappeared before initialization completed")
        })?;

        loop {
            tokio::select! {
                command = command_rx.recv() => {
                    match command {
                        Some(WorkerCommand::Insert { text, reply }) => {
                            let _ = reply.send(state.insert(&context, &text));
                        }
                        Some(WorkerCommand::Shutdown) | None => return Ok(()),
                    }
                }
                event = events.next() => {
                    let event = event
              .ok_or_else(|| {
                  InsertionError::new(
                      InsertionErrorKind::BackendUnavailable,
                      "the EIS event stream closed",
                  )
              })?
              .map_err(|error| {
                  backend_failure(format!("failed to read an EIS event: {error}"))
              })?;
                    state.handle_event(event, &context)?;
                }
            }
        }
    }

    fn monotonic_microseconds() -> u64 {
        let timestamp = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        let seconds = u64::try_from(timestamp.tv_sec).unwrap_or_default();
        let nanoseconds = u64::try_from(timestamp.tv_nsec).unwrap_or_default();
        seconds
            .saturating_mul(1_000_000)
            .saturating_add(nanoseconds / 1_000)
    }

    fn backend_failure(message: impl Into<String>) -> InsertionError {
        InsertionError::new(InsertionErrorKind::BackendFailure, message)
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::{
        InsertionError, InsertionErrorKind, InsertionReceipt, TextInserter, WaylandEisOptions,
        validate_text, wayland_eis_capability,
    };

    pub struct WaylandEisInserter;

    impl WaylandEisInserter {
        pub fn connect(_options: WaylandEisOptions) -> Result<Self, InsertionError> {
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the XDG RemoteDesktop/EIS adapter is available only on Linux",
            ))
        }

        #[must_use]
        pub const fn restore_token(&self) -> Option<&str> {
            None
        }
    }

    impl TextInserter for WaylandEisInserter {
        fn capability(&self) -> super::InsertionCapability {
            wayland_eis_capability()
        }

        fn insert_text(&mut self, text: &str) -> Result<InsertionReceipt, InsertionError> {
            validate_text(text)?;
            Err(InsertionError::new(
                InsertionErrorKind::BackendUnavailable,
                "the XDG RemoteDesktop/EIS adapter is available only on Linux",
            ))
        }
    }
}

pub use platform::WaylandEisInserter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_never_split_utf8_or_exceed_protocol_limit() {
        let text = "Türkçe🙂".repeat(80);
        let chunks: Vec<&str> = utf8_chunks(&text).collect();

        assert!(
            chunks
                .iter()
                .all(|chunk| !chunk.is_empty() && chunk.len() <= EI_TEXT_MAX_UTF8_BYTES)
        );
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn empty_and_nul_text_are_rejected_before_backend_use() {
        assert_eq!(
            validate_text("").expect_err("empty text must fail").kind(),
            InsertionErrorKind::InvalidText
        );
        assert_eq!(
            validate_text("hello\0world")
                .expect_err("NUL text must fail")
                .kind(),
            InsertionErrorKind::InvalidText
        );
    }

    #[test]
    fn capability_is_explicitly_wayland_portal_eis() {
        let capability = wayland_eis_capability();
        assert_eq!(capability.backend(), InsertionBackend::XdgRemoteDesktopEis);
        assert_eq!(
            capability.authorization(),
            InsertionAuthorization::XdgRemoteDesktop
        );
        assert!(!capability.semantic_delivery_verifiable());
    }
}
