from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"marker not found in {path}: {old[:100]!r}")
    file.write_text(text.replace(old, new, 1))


# Backend module registration and commands.
replace_once(
    "apps/desktop/src-tauri/src/lib.rs",
    "mod dictation;\nmod insertion;",
    "mod dictation;\nmod history;\nmod insertion;",
)
replace_once(
    "apps/desktop/src-tauri/src/lib.rs",
    "    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,\n    dictation_start, dictation_start_configured, insertion_capability, microphone_test_cancel,",
    "    DesktopState, audio_input_discovery, desktop_status, dictation_cancel, dictation_finish,\n    dictation_start, dictation_start_configured, history_clear, history_delete, history_list,\n    insertion_capability, microphone_test_cancel,",
)
replace_once(
    "apps/desktop/src-tauri/src/lib.rs",
    "            insertion_capability,\n            settings_get,",
    "            insertion_capability,\n            history_list,\n            history_delete,\n            history_clear,\n            settings_get,",
)

# IPC owns history beside other persistent services. History persistence is best-effort after a
# successful insertion: a disk error must never turn an already-delivered dictation into failure.
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "use crate::insertion::DesktopInsertionService;\n",
    "use crate::history::{HistoryEntry, HistoryError, HistoryService, NewHistoryEntry};\nuse crate::insertion::DesktopInsertionService;\n",
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "    insertion: Arc<DesktopInsertionService>,\n    settings: Arc<SettingsService>,",
    "    insertion: Arc<DesktopInsertionService>,\n    history: Arc<HistoryService>,\n    settings: Arc<SettingsService>,",
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "        let insertion = Arc::new(DesktopInsertionService::production());\n        let settings =\n            Arc::new(SettingsService::open(config_dir).map_err(|error| error.to_string())?);\n        let models = Arc::new(ModelManager::new(data_dir).map_err(|error| error.to_string())?);\n        Ok(Self {\n            capture,\n            dictation,\n            insertion,\n            settings,\n            models,\n        })",
    "        let insertion = Arc::new(DesktopInsertionService::production());\n        let settings =\n            Arc::new(SettingsService::open(config_dir).map_err(|error| error.to_string())?);\n        let history = Arc::new(HistoryService::open(data_dir.clone()).map_err(|error| error.to_string())?);\n        let models = Arc::new(ModelManager::new(data_dir).map_err(|error| error.to_string())?);\n        Ok(Self {\n            capture,\n            dictation,\n            insertion,\n            history,\n            settings,\n            models,\n        })",
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "        let completed = self\n            .dictation\n            .complete_insertion(session_id)\n            .map_err(CommandErrorDto::from)?;\n        Ok(DictationReportDto::completed(report, receipt, completed))",
    "        let completed = self\n            .dictation\n            .complete_insertion(session_id)\n            .map_err(CommandErrorDto::from)?;\n        let dto = DictationReportDto::completed(report, receipt, completed);\n        if let Err(error) = self.history.append(NewHistoryEntry {\n            text: dto.text.clone(),\n            detected_language: dto.detected_language.clone(),\n            engine_id: dto.engine_id.clone(),\n            model_id: dto.model_id.clone(),\n            insertion_backend: dto.insertion_backend.clone(),\n            semantic_delivery_verified: dto.semantic_delivery_verified,\n        }) {\n            eprintln!(\"BLCVoice could not persist local history after dictation completion: {error}\");\n        }\n        Ok(dto)",
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "impl From<SettingsError> for CommandErrorDto {",
    "impl From<HistoryError> for CommandErrorDto {\n    fn from(error: HistoryError) -> Self {\n        Self::plain(\"history_failed\", error.message())\n    }\n}\n\nimpl From<SettingsError> for CommandErrorDto {",
)
replace_once(
    "apps/desktop/src-tauri/src/ipc.rs",
    "#[tauri::command]\npub fn settings_get(state: State<'_, DesktopState>) -> AppSettings {",
    "#[tauri::command]\npub fn history_list(state: State<'_, DesktopState>) -> Vec<HistoryEntry> {\n    state.history.entries()\n}\n\n#[tauri::command]\npub async fn history_delete(\n    state: State<'_, DesktopState>,\n    id: u64,\n) -> Result<bool, CommandErrorDto> {\n    let history = Arc::clone(&state.history);\n    tauri::async_runtime::spawn_blocking(move || history.delete(id).map_err(CommandErrorDto::from))\n        .await\n        .map_err(|error| CommandErrorDto::blocking_worker(format!(\"history worker failed: {error}\")))?\n}\n\n#[tauri::command]\npub async fn history_clear(state: State<'_, DesktopState>) -> Result<(), CommandErrorDto> {\n    let history = Arc::clone(&state.history);\n    tauri::async_runtime::spawn_blocking(move || history.clear().map_err(CommandErrorDto::from))\n        .await\n        .map_err(|error| CommandErrorDto::blocking_worker(format!(\"history worker failed: {error}\")))?\n}\n\n#[tauri::command]\npub fn settings_get(state: State<'_, DesktopState>) -> AppSettings {",
)

# Dashboard history panel.
replace_once(
    "apps/desktop/ui/index.html",
    "      <section class=\"panel diagnostics-panel\" aria-labelledby=\"diagnostics-heading\">",
    "      <section class=\"panel history-panel\" aria-labelledby=\"history-heading\">\n        <div class=\"panel-heading\">\n          <div>\n            <p class=\"step-label\">LOCAL HISTORY</p>\n            <h2 id=\"history-heading\">Recent dictations</h2>\n          </div>\n          <button id=\"clear-history\" class=\"button secondary compact\" type=\"button\">Clear all</button>\n        </div>\n        <p class=\"panel-copy\">Up to 100 completed transcripts are stored only on this device. Raw microphone audio is never written to history.</p>\n        <div id=\"history-list\" class=\"history-list\" aria-live=\"polite\"></div>\n        <div id=\"history-message\" class=\"message muted\" role=\"status\"></div>\n      </section>\n\n      <section class=\"panel diagnostics-panel\" aria-labelledby=\"diagnostics-heading\">",
)

replace_once(
    "apps/desktop/ui/app.js",
    "  recognizerBackend: document.getElementById(\"recognizer-backend\"),\n};",
    "  recognizerBackend: document.getElementById(\"recognizer-backend\"),\n  historyList: document.getElementById(\"history-list\"),\n  historyMessage: document.getElementById(\"history-message\"),\n  clearHistory: document.getElementById(\"clear-history\"),\n};",
)
replace_once(
    "apps/desktop/ui/app.js",
    "  shortcutSessionActive: false,\n};",
    "  shortcutSessionActive: false,\n  historyBusy: false,\n};",
)
# Insert history helpers before shortcut lifecycle rendering.
replace_once(
    "apps/desktop/ui/app.js",
    "function applyShortcutLifecycle(payload) {",
    "function formatHistoryTime(unixMs) {\n  const value = Number(unixMs);\n  if (!Number.isFinite(value)) return \"Unknown time\";\n  return new Date(value).toLocaleString();\n}\n\nfunction renderHistory(entries) {\n  elements.historyList.replaceChildren();\n  const list = Array.isArray(entries) ? entries : [];\n  if (!list.length) {\n    elements.historyMessage.textContent = \"No completed dictations yet.\";\n    elements.clearHistory.disabled = true;\n    return;\n  }\n  elements.historyMessage.textContent = `${list.length} local transcript${list.length === 1 ? \"\" : \"s\"}.`;\n  elements.clearHistory.disabled = state.historyBusy;\n  for (const entry of list) {\n    const card = document.createElement(\"article\");\n    card.className = \"history-item\";\n\n    const top = document.createElement(\"div\");\n    top.className = \"history-item-top\";\n    const meta = document.createElement(\"span\");\n    const language = entry.detectedLanguage ? entry.detectedLanguage.toUpperCase() : \"auto\";\n    meta.textContent = `${formatHistoryTime(entry.createdAtUnixMs)} · ${language} · ${entry.insertionBackend}`;\n    top.append(meta);\n\n    const text = document.createElement(\"p\");\n    text.textContent = entry.text;\n\n    const actions = document.createElement(\"div\");\n    actions.className = \"history-actions\";\n    const copy = document.createElement(\"button\");\n    copy.type = \"button\";\n    copy.className = \"button secondary compact\";\n    copy.textContent = \"Copy\";\n    copy.addEventListener(\"click\", async () => {\n      try {\n        await navigator.clipboard.writeText(entry.text);\n        copy.textContent = \"Copied\";\n        window.setTimeout(() => { copy.textContent = \"Copy\"; }, 1000);\n      } catch {\n        copy.textContent = \"Unavailable\";\n      }\n    });\n    const remove = document.createElement(\"button\");\n    remove.type = \"button\";\n    remove.className = \"button danger compact\";\n    remove.textContent = \"Delete\";\n    remove.addEventListener(\"click\", () => void deleteHistoryEntry(entry.id));\n    actions.append(copy, remove);\n\n    card.append(top, text, actions);\n    elements.historyList.append(card);\n  }\n}\n\nasync function refreshHistory() {\n  if (!invoke || state.historyBusy) return;\n  try {\n    renderHistory(await invoke(\"history_list\"));\n  } catch (error) {\n    elements.historyMessage.textContent = commandErrorMessage(error);\n  }\n}\n\nasync function deleteHistoryEntry(id) {\n  if (!invoke || state.historyBusy) return;\n  state.historyBusy = true;\n  elements.clearHistory.disabled = true;\n  try {\n    await invoke(\"history_delete\", { id });\n    renderHistory(await invoke(\"history_list\"));\n  } catch (error) {\n    elements.historyMessage.textContent = commandErrorMessage(error);\n  } finally {\n    state.historyBusy = false;\n    elements.clearHistory.disabled = false;\n  }\n}\n\nasync function clearHistory() {\n  if (!invoke || state.historyBusy) return;\n  state.historyBusy = true;\n  elements.clearHistory.disabled = true;\n  try {\n    await invoke(\"history_clear\");\n    renderHistory([]);\n  } catch (error) {\n    elements.historyMessage.textContent = commandErrorMessage(error);\n  } finally {\n    state.historyBusy = false;\n  }\n}\n\nfunction applyShortcutLifecycle(payload) {",
)
# Refresh history after both explicit and shortcut completion.
replace_once(
    "apps/desktop/ui/app.js",
    "    clearDictationError();\n  } catch (error) {\n    state.dictationSessionId = null;",
    "    clearDictationError();\n    void refreshHistory();\n  } catch (error) {\n    state.dictationSessionId = null;",
)
replace_once(
    "apps/desktop/ui/app.js",
    "      elements.dictationMessage.textContent = \"Shortcut dictation completed and the transcript was submitted to the active insertion backend.\";\n      break;",
    "      elements.dictationMessage.textContent = \"Shortcut dictation completed and the transcript was submitted to the active insertion backend.\";\n      void refreshHistory();\n      break;",
)
replace_once(
    "apps/desktop/ui/app.js",
    "    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);",
    "    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), refreshHistory(), recoverDesktopStatus()]);",
)
replace_once(
    "apps/desktop/ui/app.js",
    "elements.refreshDiagnostics.addEventListener(\"click\", () => void refreshDiagnostics());\n",
    "elements.refreshDiagnostics.addEventListener(\"click\", () => void refreshDiagnostics());\nelements.clearHistory.addEventListener(\"click\", () => void clearHistory());\n",
)

# Small history-specific styles that reuse the existing button/panel system.
replace_once(
    "apps/desktop/ui/styles.css",
    ".diagnostic-grid { display: grid; grid-template-columns: repeat(3,minmax(0,1fr)); gap: 9px; }",
    ".history-list { display: grid; gap: 9px; }\n.history-item { padding: 13px; border: 1px solid var(--border); border-radius: 13px; background: var(--surface-strong); }\n.history-item-top { color: var(--muted); font-size: .69rem; font-weight: 650; }\n.history-item p { margin: 8px 0 10px; overflow: hidden; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 3; line-height: 1.5; white-space: pre-wrap; }\n.history-actions { display: flex; flex-wrap: wrap; gap: 7px; }\n\n.diagnostic-grid { display: grid; grid-template-columns: repeat(3,minmax(0,1fr)); gap: 9px; }",
)
