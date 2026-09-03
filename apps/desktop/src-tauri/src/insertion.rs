use std::sync::{Mutex, MutexGuard};

use blcvoice_insertion::{
    InsertionBackend, InsertionCapability, InsertionError, InsertionErrorKind, InsertionReceipt,
    TextInserter, resolve_insertion_capability,
};
use blcvoice_insertion_eis::{WaylandEisInserter, WaylandEisOptions};
use blcvoice_insertion_native::{NativeInserter, NativeInsertionOptions};
use blcvoice_insertion_x11::{X11Inserter, X11Options};
use blcvoice_platform::{DesktopEnvironment, current_desktop_environment};

struct InsertionState {
    inserter: Option<Box<dyn TextInserter>>,
    wayland_restore_token: Option<String>,
}

impl std::fmt::Debug for InsertionState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InsertionState")
            .field("connected", &self.inserter.is_some())
            .field(
                "has_wayland_restore_token",
                &self.wayland_restore_token.is_some(),
            )
            .finish()
    }
}

pub struct DesktopInsertionService {
    environment: DesktopEnvironment,
    state: Mutex<InsertionState>,
}

impl std::fmt::Debug for DesktopInsertionService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopInsertionService")
            .field("environment", &self.environment)
            .field("state", &*self.lock_state())
            .finish()
    }
}

impl DesktopInsertionService {
    #[must_use]
    pub fn production() -> Self {
        Self::for_environment(current_desktop_environment())
    }

    #[must_use]
    pub fn for_environment(environment: DesktopEnvironment) -> Self {
        Self {
            environment,
            state: Mutex::new(InsertionState {
                inserter: None,
                wayland_restore_token: None,
            }),
        }
    }

    pub fn capability(&self) -> Result<InsertionCapability, InsertionError> {
        resolve_insertion_capability(self.environment).map_err(|error| {
            InsertionError::new(InsertionErrorKind::BackendUnavailable, error.to_string())
        })
    }

    pub fn insert_text(&self, text: &str) -> Result<InsertionReceipt, InsertionError> {
        let capability = self.capability()?;
        let mut state = self.lock_state();
        if state.inserter.is_none() {
            state.inserter = Some(self.connect(capability.backend(), &mut state)?);
        }

        let result = state
            .inserter
            .as_mut()
            .expect("inserter must exist after connection")
            .insert_text(text);

        if result.as_ref().is_err_and(|error| {
            matches!(
                error.kind(),
                InsertionErrorKind::BackendUnavailable | InsertionErrorKind::BackendFailure
            )
        }) {
            state.inserter = None;
        }
        result
    }

    fn connect(
        &self,
        backend: InsertionBackend,
        state: &mut InsertionState,
    ) -> Result<Box<dyn TextInserter>, InsertionError> {
        match backend {
            InsertionBackend::WindowsSendInput | InsertionBackend::MacOsQuartz => {
                NativeInserter::connect(NativeInsertionOptions::default())
                    .map(|inserter| Box::new(inserter) as Box<dyn TextInserter>)
            }
            InsertionBackend::X11XTest => X11Inserter::connect(X11Options::default())
                .map(|inserter| Box::new(inserter) as Box<dyn TextInserter>),
            InsertionBackend::XdgRemoteDesktopEis => {
                let inserter = WaylandEisInserter::connect(WaylandEisOptions::new(
                    state.wayland_restore_token.clone(),
                ))?;
                state.wayland_restore_token = inserter.restore_token().map(str::to_owned);
                Ok(Box::new(inserter))
            }
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, InsertionState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use blcvoice_insertion::InsertionAuthorization;
    use blcvoice_platform::{DesktopPlatform, LinuxDisplayServer};

    use super::*;

    #[test]
    fn capability_selection_matches_environment_without_connecting() {
        let service = DesktopInsertionService::for_environment(DesktopEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Wayland,
        ));
        let capability = service
            .capability()
            .expect("Wayland capability must resolve");
        assert_eq!(capability.backend(), InsertionBackend::XdgRemoteDesktopEis);
        assert_eq!(
            capability.authorization(),
            InsertionAuthorization::XdgRemoteDesktop
        );
        assert!(!service.lock_state().inserter.is_some());
    }

    #[test]
    fn unknown_linux_environment_is_explicitly_unavailable() {
        let service = DesktopInsertionService::for_environment(DesktopEnvironment::new(
            DesktopPlatform::Linux,
            LinuxDisplayServer::Unknown,
        ));
        assert_eq!(
            service
                .capability()
                .expect_err("unknown session must fail")
                .kind(),
            InsertionErrorKind::BackendUnavailable
        );
    }
}
