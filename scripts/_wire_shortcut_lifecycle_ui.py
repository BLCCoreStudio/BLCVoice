from pathlib import Path

path = Path("apps/desktop/ui/app.js")
source = path.read_text()

source = source.replace(
    'const invoke = window.__TAURI__?.core?.invoke;\n',
    'const invoke = window.__TAURI__?.core?.invoke;\nconst listen = window.__TAURI__?.event?.listen;\n',
    1,
)

source = source.replace(
    '  dictationSessionId: null,\n  dictationBusy: false,\n',
    '  dictationSessionId: null,\n  dictationSource: null,\n  dictationBusy: false,\n  shortcutLifecycleUnlisten: null,\n',
    1,
)

old_controls = '''function updateDictationControls() {
  const active = state.dictationSessionId !== null;
  elements.dictationToggle.disabled = state.dictationBusy || (!active && !dictationReady());
  elements.dictationToggle.classList.toggle("recording", active);
  elements.dictationButtonLabel.textContent = active ? "Stop & type" : "Start dictation";
  elements.dictationCancel.hidden = !active;
  elements.dictationCancel.disabled = state.dictationBusy;
  elements.startTest.disabled = state.testBusy || Boolean(state.testSessionId) || active || !selectedDevice();
}
'''
new_controls = '''function updateDictationControls() {
  const active = state.dictationSessionId !== null;
  const coordinatorOwned = active && (state.dictationSource === "shortcut" || state.dictationSource === "recovered");
  elements.dictationToggle.disabled = state.dictationBusy || coordinatorOwned || (!active && !dictationReady());
  elements.dictationToggle.classList.toggle("recording", active);
  elements.dictationButtonLabel.textContent = coordinatorOwned
    ? "Recording via shortcut"
    : active
      ? "Stop & type"
      : "Start dictation";
  elements.dictationCancel.hidden = !active || coordinatorOwned;
  elements.dictationCancel.disabled = state.dictationBusy || coordinatorOwned;
  elements.startTest.disabled = state.testBusy || Boolean(state.testSessionId) || active || !selectedDevice();
}
'''
if old_controls not in source:
    raise SystemExit("dictation controls marker not found")
source = source.replace(old_controls, new_controls, 1)

source = source.replace(
    '    state.dictationSessionId = session.id;\n    setPill(elements.dictationState, "Recording", "recording");',
    '    state.dictationSessionId = session.id;\n    state.dictationSource = "ui";\n    setPill(elements.dictationState, "Recording", "recording");',
    1,
)
source = source.replace(
    '    state.dictationSessionId = null;\n    const language = report.detectedLanguage',
    '    state.dictationSessionId = null;\n    state.dictationSource = null;\n    const language = report.detectedLanguage',
    1,
)
source = source.replace(
    '    state.dictationSessionId = null;\n    setPill(elements.dictationState, "Failed", "failed");',
    '    state.dictationSessionId = null;\n    state.dictationSource = null;\n    setPill(elements.dictationState, "Failed", "failed");',
    1,
)
source = source.replace(
    '    state.dictationSessionId = null;\n    setPill(elements.dictationState, "Ready", "idle");\n    elements.dictationMessage.textContent = "Dictation was cancelled; captured audio was discarded.";',
    '    state.dictationSessionId = null;\n    state.dictationSource = null;\n    setPill(elements.dictationState, "Ready", "idle");\n    elements.dictationMessage.textContent = "Dictation was cancelled; captured audio was discarded.";',
    1,
)

anchor = '''async function recoverDesktopStatus() {
'''
handler = '''function handleShortcutLifecycle(event) {
  const payload = event?.payload;
  if (!payload || payload.source !== "shortcut") return;
  if (state.dictationSource === "ui" && state.dictationSessionId !== null) return;

  clearDictationError();
  switch (payload.state) {
    case "starting":
      state.dictationSource = "shortcut";
      state.dictationSessionId = null;
      state.dictationBusy = true;
      setPill(elements.dictationState, "Starting", "working");
      elements.dictationMessage.textContent = "Global shortcut is preparing the configured local recognizer…";
      break;
    case "recording":
      state.dictationSource = "shortcut";
      state.dictationSessionId = payload.sessionId ?? null;
      state.dictationBusy = false;
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent = "Listening locally. Press Ctrl + Shift + Space again to stop and type.";
      break;
    case "finishing":
      state.dictationSource = "shortcut";
      state.dictationSessionId = payload.sessionId ?? state.dictationSessionId;
      state.dictationBusy = true;
      setPill(elements.dictationState, "Transcribing", "working");
      elements.dictationMessage.textContent = "Finalizing audio, transcribing locally and delivering text to the focused app…";
      break;
    case "completed":
      state.dictationSource = null;
      state.dictationSessionId = null;
      state.dictationBusy = false;
      if (payload.text) {
        showTranscript(payload.text, `Shortcut · ${payload.insertionBackend || "insertion backend"}`);
      }
      setPill(elements.dictationState, "Ready", "passed");
      elements.dictationMessage.textContent = "Shortcut dictation completed.";
      break;
    case "failed":
      state.dictationSource = null;
      state.dictationSessionId = null;
      state.dictationBusy = false;
      setPill(elements.dictationState, "Failed", "failed");
      elements.dictationMessage.textContent = "Shortcut dictation did not complete cleanly.";
      showDictationError({
        code: payload.errorCode || "shortcut_dictation_failed",
        message: payload.message || "Shortcut dictation failed.",
        recoverableText: payload.recoverableText || null,
      });
      break;
    default:
      return;
  }
  updateDictationControls();
}

async function subscribeShortcutLifecycle() {
  if (!listen || state.shortcutLifecycleUnlisten) return;
  state.shortcutLifecycleUnlisten = await listen("blcvoice://dictation-lifecycle", handleShortcutLifecycle);
}

'''
if anchor not in source:
    raise SystemExit("recoverDesktopStatus marker not found")
source = source.replace(anchor, handler + anchor, 1)

source = source.replace(
    '      state.dictationSessionId = status.dictationSessionId;\n      setPill(elements.dictationState, "Recording", "recording");\n      elements.dictationMessage.textContent = "A dictation session was already active.";',
    '      state.dictationSessionId = status.dictationSessionId;\n      state.dictationSource = "recovered";\n      setPill(elements.dictationState, "Recording", "recording");\n      elements.dictationMessage.textContent = "A dictation session was already active. Use the global shortcut to stop it safely.";',
    1,
)
source = source.replace(
    '    } else if (status.dictationState === "idle") {\n      setPill(elements.dictationState, "Ready", "idle");',
    '    } else if (status.dictationState === "idle") {\n      state.dictationSource = null;\n      setPill(elements.dictationState, "Ready", "idle");',
    1,
)

source = source.replace(
    '  try {\n    await loadSettings();\n    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);',
    '  try {\n    await subscribeShortcutLifecycle();\n    await loadSettings();\n    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);',
    1,
)

path.write_text(source)
