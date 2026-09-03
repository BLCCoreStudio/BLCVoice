use std::sync::{Mutex, MutexGuard};

use blcvoice_core::SessionId;
use blcvoice_shortcuts::ShortcutDecision;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::ipc::{CommandErrorDto, DesktopState, DictationReportDto};
use crate::shortcut::ShortcutService;

pub const DICTATION_LIFECYCLE_EVENT: &str = "blcvoice://dictation-lifecycle";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CoordinatorState {
    #[default]
    Idle,
    Starting {
        stop_requested: bool,
    },
    Recording(SessionId),
    Finishing(SessionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartCompletion {
    Recording,
    FinishImmediately,
    CancelUnexpected,
}

#[derive(Debug, Default)]
pub struct ShortcutDictationCoordinator {
    state: Mutex<CoordinatorState>,
}

impl ShortcutDictationCoordinator {
    pub fn handle_shortcut<R: Runtime>(&self, app: AppHandle<R>, decision: ShortcutDecision) {
        match decision {
            ShortcutDecision::StartDictation => self.request_start(app),
            ShortcutDecision::StopDictation => self.request_stop(app),
            ShortcutDecision::Ignore => {}
        }
    }

    fn request_start<R: Runtime>(&self, app: AppHandle<R>) {
        let should_start = {
            let mut state = self.lock_state();
            if matches!(*state, CoordinatorState::Idle) {
                *state = CoordinatorState::Starting {
                    stop_requested: false,
                };
                true
            } else {
                false
            }
        };
        if !should_start {
            return;
        }

        emit_lifecycle(&app, DictationLifecycleEventDto::starting());
        tauri::async_runtime::spawn(async move {
            let worker_app = app.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                worker_app
                    .state::<DesktopState>()
                    .start_configured_dictation()
            })
            .await;

            match result {
                Ok(Ok(session)) => {
                    let session_id = session.id;
                    match app
                        .state::<ShortcutDictationCoordinator>()
                        .complete_start(session_id)
                    {
                        StartCompletion::Recording => {
                            emit_lifecycle(&app, DictationLifecycleEventDto::recording(session_id));
                        }
                        StartCompletion::FinishImmediately => {
                            emit_lifecycle(&app, DictationLifecycleEventDto::recording(session_id));
                            spawn_finish(app, session_id);
                        }
                        StartCompletion::CancelUnexpected => {
                            let worker_app = app.clone();
                            let _ = tauri::async_runtime::spawn_blocking(move || {
                                worker_app
                                    .state::<DesktopState>()
                                    .cancel_dictation_session(session_id)
                            })
                            .await;
                            app.state::<ShortcutDictationCoordinator>().reset();
                            app.state::<ShortcutService>().reset_controller();
                            emit_lifecycle(
                                &app,
                                DictationLifecycleEventDto::failure(
                                    Some(session_id),
                                    "coordinator_state_invalid",
                                    "dictation started after the shortcut coordinator state changed unexpectedly",
                                    None,
                                ),
                            );
                        }
                    }
                }
                Ok(Err(error)) => {
                    app.state::<ShortcutDictationCoordinator>().reset();
                    app.state::<ShortcutService>().reset_controller();
                    emit_lifecycle(&app, DictationLifecycleEventDto::from_error(None, &error));
                }
                Err(error) => {
                    app.state::<ShortcutDictationCoordinator>().reset();
                    app.state::<ShortcutService>().reset_controller();
                    emit_lifecycle(
                        &app,
                        DictationLifecycleEventDto::failure(
                            None,
                            "blocking_worker_failed",
                            format!("dictation start worker failed: {error}"),
                            None,
                        ),
                    );
                }
            }
        });
    }

    fn request_stop<R: Runtime>(&self, app: AppHandle<R>) {
        let session_to_finish = {
            let mut state = self.lock_state();
            match *state {
                CoordinatorState::Starting { .. } => {
                    *state = CoordinatorState::Starting {
                        stop_requested: true,
                    };
                    None
                }
                CoordinatorState::Recording(session_id) => {
                    *state = CoordinatorState::Finishing(session_id);
                    Some(session_id)
                }
                CoordinatorState::Idle | CoordinatorState::Finishing(_) => None,
            }
        };

        if let Some(session_id) = session_to_finish {
            spawn_finish(app, session_id);
        }
    }

    fn complete_start(&self, session_id: SessionId) -> StartCompletion {
        let mut state = self.lock_state();
        match *state {
            CoordinatorState::Starting {
                stop_requested: false,
            } => {
                *state = CoordinatorState::Recording(session_id);
                StartCompletion::Recording
            }
            CoordinatorState::Starting {
                stop_requested: true,
            } => {
                *state = CoordinatorState::Finishing(session_id);
                StartCompletion::FinishImmediately
            }
            CoordinatorState::Idle
            | CoordinatorState::Recording(_)
            | CoordinatorState::Finishing(_) => StartCompletion::CancelUnexpected,
        }
    }

    fn complete_finish(&self, session_id: SessionId) {
        let mut state = self.lock_state();
        if matches!(*state, CoordinatorState::Finishing(active_id) if active_id == session_id) {
            *state = CoordinatorState::Idle;
        }
    }

    fn reset(&self) {
        *self.lock_state() = CoordinatorState::Idle;
    }

    fn lock_state(&self) -> MutexGuard<'_, CoordinatorState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn spawn_finish<R: Runtime>(app: AppHandle<R>, session_id: SessionId) {
    emit_lifecycle(&app, DictationLifecycleEventDto::finishing(session_id));
    tauri::async_runtime::spawn(async move {
        let worker_app = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            worker_app
                .state::<DesktopState>()
                .finish_dictation_session(session_id)
        })
        .await;

        app.state::<ShortcutDictationCoordinator>()
            .complete_finish(session_id);
        match result {
            Ok(Ok(report)) => {
                emit_lifecycle(
                    &app,
                    DictationLifecycleEventDto::completed(session_id, &report),
                );
            }
            Ok(Err(error)) => {
                emit_lifecycle(
                    &app,
                    DictationLifecycleEventDto::from_error(Some(session_id), &error),
                );
            }
            Err(error) => {
                emit_lifecycle(
                    &app,
                    DictationLifecycleEventDto::failure(
                        Some(session_id),
                        "blocking_worker_failed",
                        format!("dictation finish worker failed: {error}"),
                        None,
                    ),
                );
            }
        }
    });
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DictationLifecycleEventDto {
    source: &'static str,
    state: &'static str,
    session_id: Option<u64>,
    text: Option<String>,
    insertion_backend: Option<String>,
    error_code: Option<&'static str>,
    message: Option<String>,
    recoverable_text: Option<String>,
}

impl DictationLifecycleEventDto {
    fn starting() -> Self {
        Self::state("starting", None)
    }

    fn recording(session_id: SessionId) -> Self {
        Self::state("recording", Some(session_id))
    }

    fn finishing(session_id: SessionId) -> Self {
        Self::state("finishing", Some(session_id))
    }

    fn completed(session_id: SessionId, report: &DictationReportDto) -> Self {
        Self {
            source: "shortcut",
            state: "completed",
            session_id: Some(session_id.get()),
            text: Some(report.text().to_owned()),
            insertion_backend: Some(report.insertion_backend().to_owned()),
            error_code: None,
            message: None,
            recoverable_text: None,
        }
    }

    fn from_error(session_id: Option<SessionId>, error: &CommandErrorDto) -> Self {
        Self::failure(
            session_id,
            error.code(),
            error.message(),
            error.recoverable_text().map(str::to_owned),
        )
    }

    fn failure(
        session_id: Option<SessionId>,
        error_code: &'static str,
        message: impl Into<String>,
        recoverable_text: Option<String>,
    ) -> Self {
        Self {
            source: "shortcut",
            state: "failed",
            session_id: session_id.map(SessionId::get),
            text: None,
            insertion_backend: None,
            error_code: Some(error_code),
            message: Some(message.into()),
            recoverable_text,
        }
    }

    fn state(state: &'static str, session_id: Option<SessionId>) -> Self {
        Self {
            source: "shortcut",
            state,
            session_id: session_id.map(SessionId::get),
            text: None,
            insertion_backend: None,
            error_code: None,
            message: None,
            recoverable_text: None,
        }
    }
}

fn emit_lifecycle<R: Runtime>(app: &AppHandle<R>, payload: DictationLifecycleEventDto) {
    let _ = app.emit(DICTATION_LIFECYCLE_EVENT, payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_during_start_is_queued_until_recording_exists() {
        let coordinator = ShortcutDictationCoordinator::default();
        *coordinator.lock_state() = CoordinatorState::Starting {
            stop_requested: true,
        };
        let session_id = SessionId::new(9);

        assert_eq!(
            coordinator.complete_start(session_id),
            StartCompletion::FinishImmediately
        );
        assert_eq!(
            *coordinator.lock_state(),
            CoordinatorState::Finishing(session_id)
        );
    }

    #[test]
    fn successful_start_becomes_recording() {
        let coordinator = ShortcutDictationCoordinator::default();
        *coordinator.lock_state() = CoordinatorState::Starting {
            stop_requested: false,
        };
        let session_id = SessionId::new(4);

        assert_eq!(
            coordinator.complete_start(session_id),
            StartCompletion::Recording
        );
        assert_eq!(
            *coordinator.lock_state(),
            CoordinatorState::Recording(session_id)
        );
    }

    #[test]
    fn matching_finish_returns_coordinator_to_idle() {
        let coordinator = ShortcutDictationCoordinator::default();
        let session_id = SessionId::new(11);
        *coordinator.lock_state() = CoordinatorState::Finishing(session_id);
        coordinator.complete_finish(session_id);
        assert_eq!(*coordinator.lock_state(), CoordinatorState::Idle);
    }
}
