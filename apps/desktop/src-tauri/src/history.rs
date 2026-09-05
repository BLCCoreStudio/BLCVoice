use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use blcvoice_core::SessionId;
use blcvoice_storage::{
    DeliveryState, HistoryEntry, HistoryStore, InvocationSource, NewHistoryEntry, StorageError,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::ipc::{CommandErrorDto, DesktopState, DictationReportDto};

const DEFAULT_HISTORY_LIMIT: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvocationOrigin {
    Shortcut,
    DesktopUi,
}

impl From<InvocationOrigin> for InvocationSource {
    fn from(origin: InvocationOrigin) -> Self {
        match origin {
            InvocationOrigin::Shortcut => Self::Shortcut,
            InvocationOrigin::DesktopUi => Self::DesktopUi,
        }
    }
}

#[derive(Debug)]
pub struct HistoryService {
    store: Mutex<Option<HistoryStore>>,
    database_path: PathBuf,
    last_error: Mutex<Option<String>>,
}

impl HistoryService {
    #[must_use]
    pub fn production(data_dir: PathBuf) -> Self {
        let database_path = data_dir.join("history.sqlite3");
        match HistoryStore::open(&database_path) {
            Ok(store) => Self {
                store: Mutex::new(Some(store)),
                database_path,
                last_error: Mutex::new(None),
            },
            Err(error) => Self {
                store: Mutex::new(None),
                database_path,
                last_error: Mutex::new(Some(error.to_string())),
            },
        }
    }

    fn record_report(&self, report: &DictationReportDto, origin: InvocationOrigin) {
        if !report.speech_detected() {
            return;
        }

        let result = report_history_metadata(report).and_then(|metadata| {
            self.append(NewHistoryEntry {
                created_at_unix_ms: unix_time_ms(),
                transcript: report.text().to_owned(),
                invocation_source: origin.into(),
                engine_id: metadata.engine_id,
                model_id: metadata.model_id,
                detected_language: metadata.detected_language,
                insertion_backend: report.insertion_backend().map(str::to_owned),
                delivery_state: if metadata.semantic_delivery_verified {
                    DeliveryState::DeliveredVerified
                } else {
                    DeliveryState::BackendSubmittedUnverified
                },
            })
        });
        self.capture_result(result.map(|_| ()));
    }

    fn record_failure(&self, error: &CommandErrorDto, origin: InvocationOrigin) {
        let Some(text) = error.recoverable_text() else {
            return;
        };
        if !error.code().starts_with("insertion_") {
            return;
        }

        // The legacy insertion-error DTO intentionally exposes only recoverable text.
        // Preserve that text without inventing recognizer provenance. A follow-up can
        // enrich failure metadata when the internal finish outcome carries it directly.
        let result = self.append(NewHistoryEntry {
            created_at_unix_ms: unix_time_ms(),
            transcript: text.to_owned(),
            invocation_source: origin.into(),
            engine_id: "unknown".to_owned(),
            model_id: "unknown".to_owned(),
            detected_language: None,
            insertion_backend: None,
            delivery_state: DeliveryState::InsertionFailed,
        });
        self.capture_result(result.map(|_| ()));
    }

    fn append(&self, entry: NewHistoryEntry) -> Result<HistoryEntry, StorageError> {
        let store = self.lock_store();
        let Some(store) = store.as_ref() else {
            return Err(StorageError::Database(
                self.lock_last_error()
                    .clone()
                    .unwrap_or_else(|| "history database is unavailable".to_owned()),
            ));
        };
        store.append(&entry)
    }

    fn list_recent(&self, limit: u32) -> Result<Vec<HistoryEntry>, StorageError> {
        let store = self.lock_store();
        let Some(store) = store.as_ref() else {
            return Err(StorageError::Database(
                self.lock_last_error()
                    .clone()
                    .unwrap_or_else(|| "history database is unavailable".to_owned()),
            ));
        };
        store.list_recent(limit)
    }

    fn delete(&self, id: i64) -> Result<bool, StorageError> {
        let store = self.lock_store();
        let Some(store) = store.as_ref() else {
            return Err(StorageError::Database(
                self.lock_last_error()
                    .clone()
                    .unwrap_or_else(|| "history database is unavailable".to_owned()),
            ));
        };
        store.delete(id)
    }

    fn capture_result(&self, result: Result<(), StorageError>) {
        let mut last_error = self.lock_last_error();
        match result {
            Ok(()) => *last_error = None,
            Err(error) => *last_error = Some(error.to_string()),
        }
    }

    fn health(&self) -> HistoryHealthDto {
        let store = self.lock_store();
        let last_error = self.lock_last_error().clone();
        HistoryHealthDto {
            available: store.is_some(),
            database_path: self.database_path.display().to_string(),
            last_error,
        }
    }

    fn lock_store(&self) -> MutexGuard<'_, Option<HistoryStore>> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_last_error(&self) -> MutexGuard<'_, Option<String>> {
        self.last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Debug)]
struct ReportHistoryMetadata {
    engine_id: String,
    model_id: String,
    detected_language: Option<String>,
    semantic_delivery_verified: bool,
}

fn report_history_metadata(report: &DictationReportDto) -> Result<ReportHistoryMetadata, StorageError> {
    let value = serde_json::to_value(report)
        .map_err(|error| StorageError::CorruptData(format!("could not encode dictation metadata: {error}")))?;
    Ok(ReportHistoryMetadata {
        engine_id: required_string(&value, "engineId")?,
        model_id: required_string(&value, "modelId")?,
        detected_language: optional_string(&value, "detectedLanguage")?,
        semantic_delivery_verified: value
            .get("semanticDeliveryVerified")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                StorageError::CorruptData(
                    "dictation report is missing semanticDeliveryVerified".to_owned(),
                )
            })?,
    })
}

fn required_string(value: &Value, key: &str) -> Result<String, StorageError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| StorageError::CorruptData(format!("dictation report is missing {key}")))
}

fn optional_string(value: &Value, key: &str) -> Result<Option<String>, StorageError> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(Value::String(_)) => Err(StorageError::CorruptData(format!(
            "dictation report contains blank {key}"
        ))),
        Some(_) => Err(StorageError::CorruptData(format!(
            "dictation report contains invalid {key}"
        ))),
    }
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(i64::MAX)
}

pub(crate) fn finish_and_record<R: Runtime>(
    app: &AppHandle<R>,
    session_id: SessionId,
    origin: InvocationOrigin,
) -> Result<DictationReportDto, CommandErrorDto> {
    let result = app
        .state::<DesktopState>()
        .finish_dictation_session(session_id);
    let history = app.state::<HistoryService>();
    match &result {
        Ok(report) => history.record_report(report, origin),
        Err(error) => history.record_failure(error, origin),
    }
    result
}

#[tauri::command]
pub async fn dictation_finish(
    app: tauri::AppHandle,
    session_id: u64,
) -> Result<DictationReportDto, CommandErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        finish_and_record(&app, SessionId::new(session_id), InvocationOrigin::DesktopUi)
    })
    .await
    .map_err(|error| {
        CommandErrorDto::blocking_worker(format!("desktop blocking worker failed: {error}"))
    })?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryHealthDto {
    available: bool,
    database_path: String,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryDto {
    id: i64,
    created_at_unix_ms: i64,
    transcript: String,
    invocation_source: &'static str,
    engine_id: String,
    model_id: String,
    detected_language: Option<String>,
    insertion_backend: Option<String>,
    delivery_state: &'static str,
}

impl From<HistoryEntry> for HistoryEntryDto {
    fn from(entry: HistoryEntry) -> Self {
        Self {
            id: entry.id,
            created_at_unix_ms: entry.created_at_unix_ms,
            transcript: entry.transcript,
            invocation_source: invocation_source_name(entry.invocation_source),
            engine_id: entry.engine_id,
            model_id: entry.model_id,
            detected_language: entry.detected_language,
            insertion_backend: entry.insertion_backend,
            delivery_state: delivery_state_name(entry.delivery_state),
        }
    }
}

#[tauri::command]
pub fn history_status(state: State<'_, HistoryService>) -> HistoryHealthDto {
    state.health()
}

#[tauri::command]
pub async fn history_list(
    state: State<'_, HistoryService>,
    limit: Option<u32>,
) -> Result<Vec<HistoryEntryDto>, String> {
    let limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    let entries = state
        .list_recent(limit)
        .map_err(|error| error.to_string())?;
    Ok(entries.into_iter().map(HistoryEntryDto::from).collect())
}

#[tauri::command]
pub async fn history_delete(
    state: State<'_, HistoryService>,
    id: i64,
) -> Result<bool, String> {
    state.delete(id).map_err(|error| error.to_string())
}

const fn invocation_source_name(source: InvocationSource) -> &'static str {
    match source {
        InvocationSource::Shortcut => "shortcut",
        InvocationSource::DesktopUi => "desktopUi",
    }
}

const fn delivery_state_name(state: DeliveryState) -> &'static str {
    match state {
        DeliveryState::TranscribedOnly => "transcribedOnly",
        DeliveryState::BackendSubmittedUnverified => "backendSubmittedUnverified",
        DeliveryState::DeliveredVerified => "deliveredVerified",
        DeliveryState::InsertionFailed => "insertionFailed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_mapping_preserves_shortcut_vs_ui_provenance() {
        assert_eq!(
            InvocationSource::from(InvocationOrigin::Shortcut),
            InvocationSource::Shortcut
        );
        assert_eq!(
            InvocationSource::from(InvocationOrigin::DesktopUi),
            InvocationSource::DesktopUi
        );
    }

    #[test]
    fn delivery_names_do_not_upgrade_unverified_submission() {
        assert_eq!(
            delivery_state_name(DeliveryState::BackendSubmittedUnverified),
            "backendSubmittedUnverified"
        );
        assert_eq!(
            delivery_state_name(DeliveryState::DeliveredVerified),
            "deliveredVerified"
        );
    }
}
