from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing marker: {label}")
    return text.replace(old, new, 1)

history_rs = r'''use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const HISTORY_FILE_NAME: &str = "history.json";
const HISTORY_SCHEMA_VERSION: u32 = 1;
const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    id: u64,
    created_at_unix_ms: u64,
    text: String,
    detected_language: Option<String>,
    engine_id: String,
    model_id: String,
    insertion_backend: Option<String>,
    semantic_delivery_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRecord {
    pub text: String,
    pub detected_language: Option<String>,
    pub engine_id: String,
    pub model_id: String,
    pub insertion_backend: Option<String>,
    pub semantic_delivery_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct HistoryFile {
    schema_version: u32,
    next_id: u64,
    entries: Vec<HistoryEntry>,
}

impl Default for HistoryFile {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            next_id: 1,
            entries: Vec::new(),
        }
    }
}

impl HistoryFile {
    fn normalize(&mut self) {
        self.schema_version = HISTORY_SCHEMA_VERSION;
        self.entries.retain(|entry| !entry.text.trim().is_empty());
        self.entries.truncate(MAX_HISTORY_ENTRIES);
        let minimum_next = self
            .entries
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_id = self.next_id.max(minimum_next).max(1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryError {
    message: String,
}

impl HistoryError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for HistoryError {}

#[derive(Debug)]
pub struct HistoryService {
    path: PathBuf,
    state: Mutex<HistoryFile>,
}

impl HistoryService {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self, HistoryError> {
        let data_dir = data_dir.into();
        fs::create_dir_all(&data_dir).map_err(|error| {
            HistoryError::new(format!(
                "could not create BLCVoice history directory {}: {error}",
                data_dir.display()
            ))
        })?;
        let path = data_dir.join(HISTORY_FILE_NAME);
        let mut history = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<HistoryFile>(&bytes).map_err(|error| {
                HistoryError::new(format!(
                    "could not parse BLCVoice history {}: {error}",
                    path.display()
                ))
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => HistoryFile::default(),
            Err(error) => {
                return Err(HistoryError::new(format!(
                    "could not read BLCVoice history {}: {error}",
                    path.display()
                )));
            }
        };

        if history.schema_version > HISTORY_SCHEMA_VERSION {
            return Err(HistoryError::new(format!(
                "history schema {} is newer than this BLCVoice build supports ({HISTORY_SCHEMA_VERSION})",
                history.schema_version
            )));
        }
        history.normalize();

        Ok(Self {
            path,
            state: Mutex::new(history),
        })
    }

    #[must_use]
    pub fn list(&self) -> Vec<HistoryEntry> {
        self.lock_state().entries.clone()
    }

    pub fn record(&self, record: HistoryRecord) -> Result<HistoryEntry, HistoryError> {
        let text = record.text.trim().to_owned();
        if text.is_empty() {
            return Err(HistoryError::new("history entry text must not be empty"));
        }
        let created_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| HistoryError::new(format!("system clock is before Unix epoch: {error}")))?
            .as_millis();
        let created_at_unix_ms = u64::try_from(created_at_unix_ms).unwrap_or(u64::MAX);

        let mut state = self.lock_state();
        let mut next = state.clone();
        let entry = HistoryEntry {
            id: next.next_id,
            created_at_unix_ms,
            text,
            detected_language: normalize_optional(record.detected_language),
            engine_id: record.engine_id,
            model_id: record.model_id,
            insertion_backend: normalize_optional(record.insertion_backend),
            semantic_delivery_verified: record.semantic_delivery_verified,
        };
        next.next_id = next.next_id.saturating_add(1);
        next.entries.insert(0, entry.clone());
        next.entries.truncate(MAX_HISTORY_ENTRIES);
        write_history(&self.path, &next)?;
        *state = next;
        Ok(entry)
    }

    pub fn clear(&self) -> Result<(), HistoryError> {
        let mut state = self.lock_state();
        let mut next = HistoryFile::default();
        next.next_id = state.next_id.max(1);
        write_history(&self.path, &next)?;
        *state = next;
        Ok(())
    }

    fn lock_state(&self) -> MutexGuard<'_, HistoryFile> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn write_history(path: &Path, history: &HistoryFile) -> Result<(), HistoryError> {
    let parent = path.parent().ok_or_else(|| {
        HistoryError::new(format!("history path {} has no parent directory", path.display()))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        HistoryError::new(format!(
            "could not create history directory {}: {error}",
            parent.display()
        ))
    })?;

    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(history)
        .map_err(|error| HistoryError::new(format!("could not encode history: {error}")))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| {
            HistoryError::new(format!(
                "could not open temporary history file {}: {error}",
                temporary.display()
            ))
        })?;
    file.write_all(&bytes).map_err(|error| {
        HistoryError::new(format!(
            "could not write temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    file.write_all(b"\n").map_err(|error| {
        HistoryError::new(format!(
            "could not finalize temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    file.sync_all().map_err(|error| {
        HistoryError::new(format!(
            "could not sync temporary history file {}: {error}",
            temporary.display()
        ))
    })?;
    drop(file);

    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            HistoryError::new(format!(
                "could not replace existing history file {}: {error}",
                path.display()
            ))
        })?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        HistoryError::new(format!(
            "could not commit history file {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock must be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "blcvoice-history-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn record(text: &str) -> HistoryRecord {
        HistoryRecord {
            text: text.to_owned(),
            detected_language: Some("tr".to_owned()),
            engine_id: "transcribe.cpp".to_owned(),
            model_id: "model".to_owned(),
            insertion_backend: Some("test".to_owned()),
            semantic_delivery_verified: false,
        }
    }

    #[test]
    fn history_round_trips_and_clears() {
        let directory = temporary_directory("round-trip");
        let service = HistoryService::open(&directory).expect("history must open");
        service.record(record(" merhaba ")).expect("history must save");
        let reopened = HistoryService::open(&directory).expect("history must reopen");
        assert_eq!(reopened.list().len(), 1);
        assert_eq!(reopened.list()[0].text, "merhaba");
        reopened.clear().expect("history must clear");
        assert!(reopened.list().is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn history_is_bounded() {
        let directory = temporary_directory("bounded");
        let service = HistoryService::open(&directory).expect("history must open");
        for index in 0..(MAX_HISTORY_ENTRIES + 5) {
            service
                .record(record(&format!("entry {index}")))
                .expect("history entry must save");
        }
        let entries = service.list();
        assert_eq!(entries.len(), MAX_HISTORY_ENTRIES);
        assert_eq!(entries[0].text, format!("entry {}", MAX_HISTORY_ENTRIES + 4));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn empty_history_entry_is_rejected() {
        let directory = temporary_directory("empty");
        let service = HistoryService::open(&directory).expect("history must open");
        let error = service.record(record("   ")).expect_err("empty text must fail");
        assert!(error.message().contains("must not be empty"));
        let _ = fs::remove_dir_all(directory);
    }
}
'''

history_js = r'''"use strict";

const historyInvoke = window.__TAURI__?.core?.invoke;
const historyEvent = window.__TAURI__?.event;

const historyElements = {
  enabled: document.getElementById("history-enabled"),
  state: document.getElementById("history-state"),
  message: document.getElementById("history-message"),
  list: document.getElementById("history-list"),
  clear: document.getElementById("clear-history"),
};

let historyEnabled = false;
let historyBusy = false;
let historyUnlisten = null;

function historyErrorMessage(error) {
  if (error && typeof error === "object" && typeof error.message === "string") return error.message;
  if (typeof error === "string") return error;
  return "History operation failed.";
}

function setHistoryState(label, kind = "idle") {
  historyElements.state.textContent = label;
  historyElements.state.className = `state-pill ${kind}`;
}

function historyTimestamp(unixMs) {
  const date = new Date(unixMs);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

async function copyHistoryText(text, button) {
  try {
    await navigator.clipboard.writeText(text);
    const original = button.textContent;
    button.textContent = "Copied";
    window.setTimeout(() => {
      button.textContent = original;
    }, 1200);
  } catch {
    historyElements.message.textContent = "Clipboard access was unavailable; select the transcript text manually.";
  }
}

function renderHistory(entries) {
  historyElements.list.replaceChildren();
  if (!historyEnabled) {
    historyElements.message.textContent = "History is off. BLCVoice is not storing transcript text.";
    historyElements.clear.disabled = true;
    setHistoryState("Off", "idle");
    return;
  }

  historyElements.clear.disabled = entries.length === 0 || historyBusy;
  setHistoryState(`${entries.length}/100`, entries.length ? "passed" : "idle");
  if (!entries.length) {
    historyElements.message.textContent = "No saved transcripts yet.";
    return;
  }
  historyElements.message.textContent = "Saved only on this device. Raw microphone audio is never added to history.";

  for (const entry of entries) {
    const article = document.createElement("article");
    article.className = "history-item";

    const meta = document.createElement("div");
    meta.className = "history-meta";
    const when = document.createElement("span");
    when.textContent = historyTimestamp(entry.createdAtUnixMs);
    const details = document.createElement("span");
    const language = entry.detectedLanguage ? entry.detectedLanguage.toUpperCase() : "auto";
    details.textContent = entry.insertionBackend ? `${language} · ${entry.insertionBackend}` : language;
    meta.append(when, details);

    const text = document.createElement("p");
    text.className = "history-text";
    text.textContent = entry.text;

    const actions = document.createElement("div");
    actions.className = "history-actions";
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "button secondary compact";
    copy.textContent = "Copy";
    copy.addEventListener("click", () => copyHistoryText(entry.text, copy));
    actions.append(copy);

    article.append(meta, text, actions);
    historyElements.list.append(article);
  }
}

async function refreshHistory() {
  if (!historyInvoke || historyBusy) return;
  historyBusy = true;
  try {
    const settings = await historyInvoke("settings_get");
    historyEnabled = Boolean(settings.historyEnabled);
    historyElements.enabled.checked = historyEnabled;
    const entries = historyEnabled ? await historyInvoke("history_list") : [];
    renderHistory(Array.isArray(entries) ? entries : []);
  } catch (error) {
    setHistoryState("Unavailable", "failed");
    historyElements.message.textContent = historyErrorMessage(error);
  } finally {
    historyBusy = false;
  }
}

async function setHistoryEnabled() {
  if (!historyInvoke || historyBusy) return;
  historyBusy = true;
  historyElements.enabled.disabled = true;
  try {
    const settings = await historyInvoke("settings_set_history_enabled", {
      enabled: historyElements.enabled.checked,
    });
    historyEnabled = Boolean(settings.historyEnabled);
  } catch (error) {
    historyElements.enabled.checked = historyEnabled;
    historyElements.message.textContent = historyErrorMessage(error);
  } finally {
    historyBusy = false;
    historyElements.enabled.disabled = false;
    await refreshHistory();
  }
}

async function clearHistory() {
  if (!historyInvoke || historyBusy || !historyEnabled) return;
  if (!window.confirm("Delete all locally saved BLCVoice transcripts?")) return;
  historyBusy = true;
  historyElements.clear.disabled = true;
  try {
    await historyInvoke("history_clear");
  } catch (error) {
    historyElements.message.textContent = historyErrorMessage(error);
  } finally {
    historyBusy = false;
    await refreshHistory();
  }
}

async function initializeHistory() {
  if (!historyInvoke) {
    setHistoryState("Unavailable", "failed");
    historyElements.message.textContent = "The desktop bridge is unavailable.";
    historyElements.enabled.disabled = true;
    historyElements.clear.disabled = true;
    return;
  }
  historyElements.enabled.addEventListener("change", setHistoryEnabled);
  historyElements.clear.addEventListener("click", clearHistory);
  window.addEventListener("blcvoice-history-refresh", refreshHistory);
  window.addEventListener("beforeunload", () => {
    if (historyUnlisten) historyUnlisten();
  });
  if (historyEvent?.listen) {
    historyUnlisten = await historyEvent.listen("blcvoice://dictation-lifecycle", (event) => {
      const state = event?.payload?.state;
      if (state === "completed" || state === "noSpeech") void refreshHistory();
    });
  }
  await refreshHistory();
}

void initializeHistory();
'''

Path("apps/desktop/src-tauri/src/history.rs").write_text(history_rs)
Path("apps/desktop/ui/history.js").write_text(history_js)

settings_path = Path("apps/desktop/src-tauri/src/settings.rs")
settings = settings_path.read_text()
settings = replace_once(settings, "const SETTINGS_SCHEMA_VERSION: u32 = 1;", "const SETTINGS_SCHEMA_VERSION: u32 = 2;", "settings schema")
settings = replace_once(
    settings,
    "    language_hint: Option<String>,\n}",
    "    language_hint: Option<String>,\n    history_enabled: bool,\n}",
    "history settings field",
)
settings = replace_once(
    settings,
    "            language_hint: None,\n        }",
    "            language_hint: None,\n            history_enabled: false,\n        }",
    "history default",
)
settings = replace_once(
    settings,
    "    pub fn language_hint(&self) -> Option<&str> {\n        self.language_hint.as_deref()\n    }\n",
    "    pub fn language_hint(&self) -> Option<&str> {\n        self.language_hint.as_deref()\n    }\n\n    #[must_use]\n    pub const fn history_enabled(&self) -> bool {\n        self.history_enabled\n    }\n",
    "history getter",
)
settings = replace_once(
    settings,
    "    pub fn set_language_hint(\n        &self,\n        language_hint: Option<String>,\n    ) -> Result<AppSettings, SettingsError> {\n        self.update(|settings| settings.language_hint = language_hint)\n    }\n",
    "    pub fn set_language_hint(\n        &self,\n        language_hint: Option<String>,\n    ) -> Result<AppSettings, SettingsError> {\n        self.update(|settings| settings.language_hint = language_hint)\n    }\n\n    pub fn set_history_enabled(&self, enabled: bool) -> Result<AppSettings, SettingsError> {\n        self.update(|settings| settings.history_enabled = enabled)\n    }\n",
    "history setter",
)
settings = replace_once(
    settings,
    "        assert_eq!(settings.language_hint(), Some(\"tr\"));\n",
    "        assert_eq!(settings.language_hint(), Some(\"tr\"));\n        assert!(!settings.history_enabled());\n",
    "history default assertion",
)
settings = replace_once(
    settings,
    "    #[test]\n    fn auto_language_is_stored_as_no_hint() {",
    "    #[test]\n    fn history_preference_round_trips() {\n        let directory = temporary_directory(\"history-setting\");\n        let service = SettingsService::open(&directory).expect(\"settings service must open\");\n        let settings = service\n            .set_history_enabled(true)\n            .expect(\"history preference must save\");\n        assert!(settings.history_enabled());\n        let reopened = SettingsService::open(&directory).expect(\"settings must reopen\");\n        assert!(reopened.snapshot().history_enabled());\n        let _ = fs::remove_dir_all(directory);\n    }\n\n    #[test]\n    fn auto_language_is_stored_as_no_hint() {",
    "history settings test",
)
settings_path.write_text(settings)

ipc_path = Path("apps/desktop/src-tauri/src/ipc.rs")
ipc = ipc_path.read_text()
ipc = replace_once(
    ipc,
    "use crate::insertion::DesktopInsertionService;\n",
    "use crate::history::{HistoryEntry, HistoryError, HistoryRecord, HistoryService};\nuse crate::insertion::DesktopInsertionService;\n",
    "history import",
)
ipc = replace_once(
    ipc,
    "    settings: Arc<SettingsService>,\n    models: Arc<ModelManager>,\n}",
    "    settings: Arc<SettingsService>,\n    models: Arc<ModelManager>,\n    history: Arc<HistoryService>,\n}",
    "history state field",
)
ipc = replace_once(
    ipc,
    "        let models = Arc::new(ModelManager::new(data_dir).map_err(|error| error.to_string())?);\n        Ok(Self {\n            capture,\n            dictation,\n            insertion,\n            settings,\n            models,\n        })",
    "        let history = Arc::new(\n            HistoryService::open(data_dir.join(\"history\")).map_err(|error| error.to_string())?,\n        );\n        let models = Arc::new(ModelManager::new(data_dir).map_err(|error| error.to_string())?);\n        Ok(Self {\n            capture,\n            dictation,\n            insertion,\n            settings,\n            models,\n            history,\n        })",
    "history production state",
)
ipc = replace_once(
    ipc,
    "        let completed = self\n            .dictation\n            .complete_insertion(session_id)\n            .map_err(CommandErrorDto::from)?;\n        Ok(DictationReportDto::completed(report, receipt, completed))",
    "        let completed = self\n            .dictation\n            .complete_insertion(session_id)\n            .map_err(CommandErrorDto::from)?;\n        let mut dto = DictationReportDto::completed(report, receipt, completed);\n        dto.history_enabled = self.settings.snapshot().history_enabled();\n        if dto.history_enabled {\n            dto.history_saved = self\n                .history\n                .record(HistoryRecord {\n                    text: dto.text.clone(),\n                    detected_language: dto.detected_language.clone(),\n                    engine_id: dto.engine_id.clone(),\n                    model_id: dto.model_id.clone(),\n                    insertion_backend: dto.insertion_backend.clone(),\n                    semantic_delivery_verified: dto.semantic_delivery_verified,\n                })\n                .is_ok();\n        }\n        Ok(dto)",
    "record successful transcript",
)
ipc = replace_once(
    ipc,
    "impl From<ModelError> for CommandErrorDto {",
    "impl From<HistoryError> for CommandErrorDto {\n    fn from(error: HistoryError) -> Self {\n        Self::plain(\"history_failed\", error.message())\n    }\n}\n\nimpl From<ModelError> for CommandErrorDto {",
    "history error dto",
)
ipc = replace_once(
    ipc,
    "    semantic_delivery_verified: bool,\n}",
    "    semantic_delivery_verified: bool,\n    history_enabled: bool,\n    history_saved: bool,\n}",
    "dictation history dto fields",
)
ipc = replace_once(
    ipc,
    "            semantic_delivery_verified: receipt.semantic_delivery_verified(),\n        }",
    "            semantic_delivery_verified: receipt.semantic_delivery_verified(),\n            history_enabled: false,\n            history_saved: false,\n        }",
    "completed history fields",
)
ipc = replace_once(
    ipc,
    "            semantic_delivery_verified: false,\n        }\n    }\n}",
    "            semantic_delivery_verified: false,\n            history_enabled: false,\n            history_saved: false,\n        }\n    }\n}",
    "no speech history fields",
)
ipc = replace_once(
    ipc,
    "#[tauri::command]\npub fn settings_get(state: State<'_, DesktopState>) -> AppSettings {\n    state.settings.snapshot()\n}\n",
    "#[tauri::command]\npub fn settings_get(state: State<'_, DesktopState>) -> AppSettings {\n    state.settings.snapshot()\n}\n\n#[tauri::command]\npub fn history_list(state: State<'_, DesktopState>) -> Vec<HistoryEntry> {\n    state.history.list()\n}\n\n#[tauri::command]\npub fn history_clear(state: State<'_, DesktopState>) -> Result<(), CommandErrorDto> {\n    state.history.clear().map_err(CommandErrorDto::from)\n}\n",
    "history commands",
)
ipc = replace_once(
    ipc,
    "#[tauri::command]\npub fn settings_set_language_hint(\n    state: State<'_, DesktopState>,\n    language_hint: Option<String>,\n) -> Result<AppSettings, CommandErrorDto> {\n    state\n        .settings\n        .set_language_hint(language_hint)\n        .map_err(CommandErrorDto::from)\n}\n",
    "#[tauri::command]\npub fn settings_set_language_hint(\n    state: State<'_, DesktopState>,\n    language_hint: Option<String>,\n) -> Result<AppSettings, CommandErrorDto> {\n    state\n        .settings\n        .set_language_hint(language_hint)\n        .map_err(CommandErrorDto::from)\n}\n\n#[tauri::command]\npub fn settings_set_history_enabled(\n    state: State<'_, DesktopState>,\n    enabled: bool,\n) -> Result<AppSettings, CommandErrorDto> {\n    state\n        .settings\n        .set_history_enabled(enabled)\n        .map_err(CommandErrorDto::from)\n}\n",
    "history setting command",
)
ipc_path.write_text(ipc)

lib_path = Path("apps/desktop/src-tauri/src/lib.rs")
lib = lib_path.read_text()
lib = replace_once(lib, "mod insertion;\n", "mod history;\nmod insertion;\n", "history module")
lib = replace_once(
    lib,
    "    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,\n    dictation_start, dictation_start_configured, insertion_capability, microphone_test_cancel,\n",
    "    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,\n    dictation_start, dictation_start_configured, history_clear, history_list, insertion_capability,\n    microphone_test_cancel,\n",
    "history command imports",
)
lib = replace_once(
    lib,
    "    settings_get, settings_set_input_device, settings_set_language_hint, settings_set_model,\n",
    "    settings_get, settings_set_history_enabled, settings_set_input_device,\n    settings_set_language_hint, settings_set_model,\n",
    "history settings import",
)
lib = replace_once(
    lib,
    "            insertion_capability,\n            settings_get,\n",
    "            insertion_capability,\n            history_list,\n            history_clear,\n            settings_get,\n",
    "history handlers",
)
lib = replace_once(
    lib,
    "            settings_set_language_hint,\n            model_catalog,\n",
    "            settings_set_language_hint,\n            settings_set_history_enabled,\n            model_catalog,\n",
    "history setting handler",
)
lib_path.write_text(lib)

index_path = Path("apps/desktop/ui/index.html")
index = index_path.read_text()
index = replace_once(index, "    <script src=\"app.js\" defer></script>\n", "    <script src=\"app.js\" defer></script>\n    <script src=\"history.js\" defer></script>\n", "history script")
index = replace_once(
    index,
    "          <div id=\"settings-message\" class=\"message muted\" role=\"status\"></div>\n        </section>",
    "          <div class=\"privacy-setting\">\n            <label for=\"history-enabled\" class=\"toggle-label\">\n              <input id=\"history-enabled\" type=\"checkbox\" />\n              <span>Save transcript history</span>\n            </label>\n            <small>Off by default. Stores up to 100 transcripts locally; microphone audio is never saved.</small>\n          </div>\n          <div id=\"settings-message\" class=\"message muted\" role=\"status\"></div>\n        </section>",
    "history privacy control",
)
index = replace_once(
    index,
    "      <section class=\"panel diagnostics-panel\" aria-labelledby=\"diagnostics-heading\">",
    "      <section class=\"panel history-panel\" aria-labelledby=\"history-heading\">\n        <div class=\"panel-heading\">\n          <div>\n            <p class=\"step-label\">HISTORY</p>\n            <h2 id=\"history-heading\">Local transcripts</h2>\n          </div>\n          <div class=\"history-header-actions\">\n            <span id=\"history-state\" class=\"state-pill idle\">Off</span>\n            <button id=\"clear-history\" class=\"button secondary compact\" type=\"button\" disabled>Clear</button>\n          </div>\n        </div>\n        <p id=\"history-message\" class=\"panel-copy\">History is off. BLCVoice is not storing transcript text.</p>\n        <div id=\"history-list\" class=\"history-list\" aria-live=\"polite\"></div>\n      </section>\n\n      <section class=\"panel diagnostics-panel\" aria-labelledby=\"diagnostics-heading\">",
    "history panel",
)
index_path.write_text(index)

app_path = Path("apps/desktop/ui/app.js")
app = app_path.read_text()
app = replace_once(
    app,
    "    clearDictationError();\n  } catch (error) {\n    state.dictationSessionId = null;",
    "    clearDictationError();\n    window.dispatchEvent(new Event(\"blcvoice-history-refresh\"));\n    if (report.historyEnabled && !report.historySaved) {\n      elements.settingsMessage.textContent = \"Dictation completed, but the local history entry could not be saved.\";\n    }\n  } catch (error) {\n    state.dictationSessionId = null;",
    "history refresh after UI dictation",
)
app_path.write_text(app)

styles_path = Path("apps/desktop/ui/styles.css")
styles = styles_path.read_text()
styles += r'''

.privacy-setting {
  margin-top: 18px;
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--surface-subtle);
}

.toggle-label {
  display: flex;
  align-items: center;
  gap: 10px;
  font-weight: 700;
  cursor: pointer;
}

.toggle-label input {
  width: 18px;
  height: 18px;
  accent-color: currentColor;
}

.privacy-setting small {
  display: block;
  margin-top: 7px;
  color: var(--muted);
  line-height: 1.45;
}

.history-header-actions,
.history-meta,
.history-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.history-list {
  display: grid;
  gap: 10px;
}

.history-item {
  padding: 14px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--surface-subtle);
}

.history-meta {
  justify-content: space-between;
  color: var(--muted);
  font-size: 0.78rem;
}

.history-text {
  margin: 10px 0 12px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  line-height: 1.5;
}

.history-actions {
  justify-content: flex-end;
}
'''
styles_path.write_text(styles)

ci_path = Path(".github/workflows/ci.yml")
ci = ci_path.read_text()
ci = replace_once(
    ci,
    "          node --check apps/desktop/ui/app.js\n          node --check apps/desktop/ui/overlay.js\n",
    "          node --check apps/desktop/ui/app.js\n          node --check apps/desktop/ui/history.js\n          node --check apps/desktop/ui/overlay.js\n",
    "history javascript CI",
)
ci_path.write_text(ci)
