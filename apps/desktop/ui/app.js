"use strict";

const AUTO_FINISH_MS = 10_000;
const invoke = window.__TAURI__?.core?.invoke;

const elements = {
  dictationState: document.getElementById("dictation-state"),
  dictationMessage: document.getElementById("dictation-message"),
  dictationToggle: document.getElementById("dictation-toggle"),
  dictationButtonLabel: document.getElementById("dictation-button-label"),
  dictationCancel: document.getElementById("dictation-cancel"),
  dictationError: document.getElementById("dictation-error"),
  transcriptCard: document.getElementById("transcript-card"),
  transcriptText: document.getElementById("transcript-text"),
  transcriptMeta: document.getElementById("transcript-meta"),
  copyTranscript: document.getElementById("copy-transcript"),
  modelList: document.getElementById("model-list"),
  modelState: document.getElementById("model-state"),
  modelMessage: document.getElementById("model-message"),
  deviceSelect: document.getElementById("device-select"),
  refreshDevices: document.getElementById("refresh-devices"),
  deviceBackend: document.getElementById("device-backend"),
  deviceFormat: document.getElementById("device-format"),
  languageSelect: document.getElementById("language-select"),
  settingsMessage: document.getElementById("settings-message"),
  testState: document.getElementById("test-state"),
  testInstruction: document.getElementById("test-instruction"),
  testProgressWrap: document.getElementById("test-progress-wrap"),
  testProgress: document.getElementById("test-progress"),
  testTimer: document.getElementById("test-timer"),
  startTest: document.getElementById("start-test"),
  finishTest: document.getElementById("finish-test"),
  cancelTest: document.getElementById("cancel-test"),
  resultCard: document.getElementById("result-card"),
  resultTitle: document.getElementById("result-title"),
  resultBadge: document.getElementById("result-badge"),
  resultSummary: document.getElementById("result-summary"),
  resultFrames: document.getElementById("result-frames"),
  resultDropped: document.getElementById("result-dropped"),
  resultErrors: document.getElementById("result-errors"),
  refreshDiagnostics: document.getElementById("refresh-diagnostics"),
  shortcutBackend: document.getElementById("shortcut-backend"),
  shortcutDetail: document.getElementById("shortcut-detail"),
  insertionBackend: document.getElementById("insertion-backend"),
  insertionDetail: document.getElementById("insertion-detail"),
  recognizerDetail: document.getElementById("recognizer-detail"),
  recognizerBackend: document.getElementById("recognizer-backend"),
};

const state = {
  devices: [],
  settings: null,
  models: [],
  dictationSessionId: null,
  dictationBusy: false,
  modelBusyId: null,
  settingsBusy: false,
  testSessionId: null,
  testBusy: false,
  testStartedAt: null,
  testTimerId: null,
  testAutoFinishStarted: false,
};

function commandErrorMessage(error) {
  if (error && typeof error === "object") {
    const message = typeof error.message === "string" ? error.message : null;
    const code = typeof error.code === "string" ? error.code : null;
    if (message && code) return `${message} (${code})`;
    if (message) return message;
  }
  if (typeof error === "string") return error;
  return "The desktop bridge returned an unknown error.";
}

function commandRecoverableText(error) {
  return error && typeof error === "object" && typeof error.recoverableText === "string"
    ? error.recoverableText
    : null;
}

function setPill(element, label, kind = "idle") {
  element.textContent = label;
  element.className = `state-pill ${kind}`;
}

function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

function formatNativeConfig(config) {
  if (!config) return "Unavailable";
  const channels = config.channels === 1 ? "mono" : `${config.channels} ch`;
  const rate = `${(config.sampleRateHz / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 })} kHz`;
  return `${rate} · ${channels}`;
}

function selectedDevice() {
  return state.devices.find((device) => device.id === elements.deviceSelect.value) ?? null;
}

function installedModelAvailable() {
  return state.models.some((model) => model.installed);
}

function selectedOrFallbackModel() {
  return (
    state.models.find((model) => model.selected && model.installed) ??
    state.models.find((model) => model.recommended && model.installed) ??
    state.models.find((model) => model.installed) ??
    null
  );
}

function dictationReady() {
  return Boolean(invoke && selectedDevice() && installedModelAvailable() && !state.testSessionId);
}

function updateDictationControls() {
  const active = state.dictationSessionId !== null;
  elements.dictationToggle.disabled = state.dictationBusy || (!active && !dictationReady());
  elements.dictationToggle.classList.toggle("recording", active);
  elements.dictationButtonLabel.textContent = active ? "Stop & type" : "Start dictation";
  elements.dictationCancel.hidden = !active;
  elements.dictationCancel.disabled = state.dictationBusy;
  elements.startTest.disabled = state.testBusy || Boolean(state.testSessionId) || active || !selectedDevice();
}

function showDictationError(error) {
  const recoverable = commandRecoverableText(error);
  elements.dictationError.textContent = commandErrorMessage(error);
  elements.dictationError.hidden = false;
  if (recoverable) showTranscript(recoverable, "Insertion failed · text recovered");
}

function clearDictationError() {
  elements.dictationError.hidden = true;
  elements.dictationError.textContent = "";
}

function showTranscript(text, meta = "") {
  elements.transcriptText.textContent = text || "";
  elements.transcriptMeta.textContent = meta;
  elements.transcriptCard.hidden = !text;
}

async function startDictation() {
  if (!invoke || state.dictationBusy || state.dictationSessionId !== null || !dictationReady()) return;
  state.dictationBusy = true;
  clearDictationError();
  setPill(elements.dictationState, "Starting", "working");
  elements.dictationMessage.textContent = "Preparing the local recognizer before opening your microphone…";
  updateDictationControls();
  try {
    const session = await invoke("dictation_start_configured");
    state.dictationSessionId = session.id;
    setPill(elements.dictationState, "Recording", "recording");
    elements.dictationMessage.textContent = "Listening locally. Press the button or Ctrl + Shift + Space when you are done.";
  } catch (error) {
    setPill(elements.dictationState, "Unavailable", "failed");
    elements.dictationMessage.textContent = "Dictation could not start.";
    showDictationError(error);
  } finally {
    state.dictationBusy = false;
    updateDictationControls();
  }
}

async function finishDictation() {
  const sessionId = state.dictationSessionId;
  if (!invoke || state.dictationBusy || sessionId === null) return;
  state.dictationBusy = true;
  setPill(elements.dictationState, "Transcribing", "working");
  elements.dictationMessage.textContent = "Finalizing audio, transcribing locally and delivering text to the focused app…";
  updateDictationControls();
  try {
    const report = await invoke("dictation_finish", { sessionId });
    state.dictationSessionId = null;
    const language = report.detectedLanguage ? report.detectedLanguage.toUpperCase() : "auto";
    showTranscript(report.text, `${language} · ${report.insertionBackend}`);
    elements.recognizerDetail.textContent = report.modelId || "Loaded model";
    elements.recognizerBackend.textContent = `${report.engineId} · ${report.backendName}`;
    setPill(elements.dictationState, "Ready", "passed");
    elements.dictationMessage.textContent = report.semanticDeliveryVerified
      ? "Text delivery was verified by the active backend."
      : "The complete transcript was submitted to the operating-system insertion backend.";
    clearDictationError();
  } catch (error) {
    state.dictationSessionId = null;
    setPill(elements.dictationState, "Failed", "failed");
    elements.dictationMessage.textContent = "The dictation pipeline stopped before a clean completion.";
    showDictationError(error);
  } finally {
    state.dictationBusy = false;
    updateDictationControls();
  }
}

async function cancelDictation() {
  const sessionId = state.dictationSessionId;
  if (!invoke || state.dictationBusy || sessionId === null) return;
  state.dictationBusy = true;
  updateDictationControls();
  try {
    await invoke("dictation_cancel", { sessionId });
    state.dictationSessionId = null;
    setPill(elements.dictationState, "Ready", "idle");
    elements.dictationMessage.textContent = "Dictation was cancelled; captured audio was discarded.";
    clearDictationError();
  } catch (error) {
    showDictationError(error);
  } finally {
    state.dictationBusy = false;
    updateDictationControls();
  }
}

async function toggleDictation() {
  if (state.dictationSessionId === null) await startDictation();
  else await finishDictation();
}

async function loadSettings() {
  state.settings = await invoke("settings_get");
  elements.languageSelect.value = state.settings.languageHint || "auto";
}

function renderSelectedDevice() {
  const device = selectedDevice();
  elements.deviceBackend.textContent = device?.backend || "—";
  elements.deviceFormat.textContent = formatNativeConfig(device?.defaultConfig);
  updateDictationControls();
}

async function refreshDevices() {
  if (!invoke || state.settingsBusy || state.testSessionId || state.dictationSessionId) return;
  state.settingsBusy = true;
  elements.refreshDevices.disabled = true;
  try {
    const discovery = await invoke("audio_input_discovery");
    state.devices = Array.isArray(discovery.devices) ? discovery.devices : [];
    elements.deviceSelect.replaceChildren();
    if (!state.devices.length) {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = "No usable microphone found";
      elements.deviceSelect.append(option);
      elements.settingsMessage.textContent = "No microphone is currently available.";
    } else {
      for (const device of state.devices) {
        const option = document.createElement("option");
        option.value = device.id;
        option.textContent = device.isDefault ? `${device.name} — Default` : device.name;
        elements.deviceSelect.append(option);
      }
      const configured = state.settings?.selectedInputDeviceId;
      const choice =
        state.devices.find((device) => device.id === configured) ??
        state.devices.find((device) => device.isDefault) ??
        state.devices[0];
      elements.deviceSelect.value = choice.id;
      if (configured !== choice.id) {
        state.settings = await invoke("settings_set_input_device", { deviceId: choice.id });
      }
      const failures = Array.isArray(discovery.failures) ? discovery.failures : [];
      elements.settingsMessage.textContent = failures.length
        ? `Microphone discovery reported ${failures.length} warning${failures.length === 1 ? "" : "s"}.`
        : `${state.devices.length} input device${state.devices.length === 1 ? "" : "s"} available.`;
    }
    elements.deviceSelect.disabled = !state.devices.length;
    renderSelectedDevice();
  } catch (error) {
    state.devices = [];
    elements.deviceSelect.disabled = true;
    elements.settingsMessage.textContent = commandErrorMessage(error);
    renderSelectedDevice();
  } finally {
    state.settingsBusy = false;
    elements.refreshDevices.disabled = false;
    updateDictationControls();
  }
}

async function saveSelectedDevice() {
  const device = selectedDevice();
  if (!device || state.settingsBusy) return;
  state.settingsBusy = true;
  elements.deviceSelect.disabled = true;
  try {
    state.settings = await invoke("settings_set_input_device", { deviceId: device.id });
    elements.settingsMessage.textContent = "Microphone selection saved.";
  } catch (error) {
    elements.settingsMessage.textContent = commandErrorMessage(error);
  } finally {
    state.settingsBusy = false;
    elements.deviceSelect.disabled = !state.devices.length;
    renderSelectedDevice();
  }
}

async function saveLanguage() {
  if (!invoke || state.settingsBusy) return;
  state.settingsBusy = true;
  elements.languageSelect.disabled = true;
  try {
    const value = elements.languageSelect.value;
    state.settings = await invoke("settings_set_language_hint", {
      languageHint: value === "auto" ? null : value,
    });
    elements.settingsMessage.textContent = value === "auto" ? "Automatic language detection enabled." : "Recognition language saved.";
  } catch (error) {
    elements.settingsMessage.textContent = commandErrorMessage(error);
  } finally {
    state.settingsBusy = false;
    elements.languageSelect.disabled = false;
  }
}

function createModelButton(label, className, action, disabled = false) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.textContent = label;
  button.disabled = disabled;
  button.addEventListener("click", action);
  return button;
}

function renderModels() {
  elements.modelList.replaceChildren();
  for (const model of state.models) {
    const card = document.createElement("article");
    card.className = `model-card${model.selected ? " selected" : ""}`;
    const top = document.createElement("div");
    top.className = "model-card-top";
    const title = document.createElement("strong");
    title.textContent = model.name;
    top.append(title);
    if (model.recommended) {
      const badge = document.createElement("span");
      badge.className = "model-badge recommended";
      badge.textContent = "Recommended";
      top.append(badge);
    } else if (model.installed) {
      const badge = document.createElement("span");
      badge.className = "model-badge installed";
      badge.textContent = model.selected ? "Selected" : "Installed";
      top.append(badge);
    }
    card.append(top);
    const description = document.createElement("p");
    description.textContent = model.description;
    card.append(description);
    const meta = document.createElement("div");
    meta.className = "model-meta";
    meta.textContent = `${model.tier} · ${formatBytes(model.installedBytes || model.advertisedBytes)}`;
    card.append(meta);
    const actions = document.createElement("div");
    actions.className = "model-actions";
    const busy = state.modelBusyId !== null;
    if (!model.installed) {
      actions.append(createModelButton(
        state.modelBusyId === model.id ? "Installing…" : "Install",
        "button primary compact",
        () => void installModel(model.id),
        busy,
      ));
    } else {
      if (!model.selected) {
        actions.append(createModelButton("Use model", "button secondary compact", () => void selectModel(model.id), busy));
      }
      actions.append(createModelButton("Remove", "button danger compact", () => void removeModel(model.id), busy || model.selected));
    }
    card.append(actions);
    elements.modelList.append(card);
  }
  const selected = selectedOrFallbackModel();
  if (selected) {
    setPill(elements.modelState, selected.selected ? "Selected" : "Installed", "passed");
    elements.modelMessage.textContent = selected.selected
      ? `${selected.name} is ready for local dictation.`
      : `${selected.name} is installed and can be selected.`;
  } else {
    setPill(elements.modelState, "Required", "failed");
    elements.modelMessage.textContent = "Install one local model before starting dictation.";
  }
  updateDictationControls();
}

async function refreshModels() {
  try {
    state.models = await invoke("model_catalog");
    renderModels();
  } catch (error) {
    setPill(elements.modelState, "Unavailable", "failed");
    elements.modelMessage.textContent = commandErrorMessage(error);
  }
}

async function installModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
  state.modelBusyId = modelId;
  setPill(elements.modelState, "Installing", "working");
  elements.modelMessage.textContent = "Downloading and validating the model locally…";
  renderModels();
  try {
    await invoke("model_install", { modelId });
    state.settings = await invoke("settings_get");
    await refreshModels();
  } catch (error) {
    setPill(elements.modelState, "Failed", "failed");
    elements.modelMessage.textContent = commandErrorMessage(error);
  } finally {
    state.modelBusyId = null;
    await refreshModels();
  }
}

async function selectModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
  state.modelBusyId = modelId;
  renderModels();
  try {
    state.settings = await invoke("settings_set_model", { modelId });
  } catch (error) {
    elements.modelMessage.textContent = commandErrorMessage(error);
  } finally {
    state.modelBusyId = null;
    await refreshModels();
  }
}

async function removeModel(modelId) {
  if (!invoke || state.modelBusyId !== null) return;
  state.modelBusyId = modelId;
  renderModels();
  try {
    await invoke("model_remove", { modelId });
  } catch (error) {
    elements.modelMessage.textContent = commandErrorMessage(error);
  } finally {
    state.modelBusyId = null;
    await refreshModels();
  }
}

function stopTestTimer() {
  if (state.testTimerId !== null) {
    window.clearInterval(state.testTimerId);
    state.testTimerId = null;
  }
}

function renderTestTimer() {
  if (state.testStartedAt === null) return;
  const elapsed = Math.max(0, Date.now() - state.testStartedAt);
  const seconds = Math.min(AUTO_FINISH_MS, elapsed) / 1000;
  elements.testTimer.textContent = `${seconds.toFixed(1)}s`;
  elements.testProgress.value = seconds;
  if (elapsed >= AUTO_FINISH_MS && state.testSessionId !== null && !state.testBusy && !state.testAutoFinishStarted) {
    state.testAutoFinishStarted = true;
    void finishTest(true);
  }
}

function resetTest() {
  stopTestTimer();
  state.testSessionId = null;
  state.testStartedAt = null;
  state.testAutoFinishStarted = false;
  elements.testProgressWrap.hidden = true;
  elements.testProgress.value = 0;
  elements.testTimer.textContent = "0.0s";
  elements.startTest.hidden = false;
  elements.finishTest.hidden = true;
  elements.cancelTest.hidden = true;
  updateDictationControls();
}

async function startTest() {
  const device = selectedDevice();
  if (!invoke || !device || state.testBusy || state.testSessionId || state.dictationSessionId) return;
  state.testBusy = true;
  setPill(elements.testState, "Starting", "working");
  try {
    const session = await invoke("microphone_test_start", { deviceId: device.id });
    state.testSessionId = session.id;
    state.testStartedAt = Date.now();
    state.testAutoFinishStarted = false;
    elements.testProgressWrap.hidden = false;
    elements.startTest.hidden = true;
    elements.finishTest.hidden = false;
    elements.cancelTest.hidden = false;
    setPill(elements.testState, "Recording", "recording");
    elements.testInstruction.textContent = "Speak normally. The check stops automatically after 10 seconds.";
    renderTestTimer();
    state.testTimerId = window.setInterval(renderTestTimer, 100);
  } catch (error) {
    setPill(elements.testState, "Failed", "failed");
    elements.testInstruction.textContent = commandErrorMessage(error);
  } finally {
    state.testBusy = false;
    updateDictationControls();
  }
}

async function finishTest(automatic = false) {
  const sessionId = state.testSessionId;
  if (!invoke || sessionId === null || state.testBusy) return;
  state.testBusy = true;
  stopTestTimer();
  setPill(elements.testState, "Validating", "working");
  try {
    const report = await invoke("microphone_test_finish", { sessionId });
    resetTest();
    const stats = report.captureStats;
    elements.resultTitle.textContent = "Capture validated";
    elements.resultBadge.textContent = "Passed";
    elements.resultBadge.className = "result-badge success";
    elements.resultFrames.textContent = Number(report.capturedFrames).toLocaleString();
    elements.resultDropped.textContent = Number(stats.droppedSamples).toLocaleString();
    elements.resultErrors.textContent = Number(stats.callbackErrors).toLocaleString();
    elements.resultSummary.textContent = automatic
      ? "The 10-second check completed cleanly; test audio was discarded."
      : "The native stream finalized cleanly; test audio was discarded.";
    elements.resultCard.hidden = false;
    setPill(elements.testState, "Passed", "passed");
    elements.testInstruction.textContent = "This microphone passed the native capture check.";
  } catch (error) {
    resetTest();
    setPill(elements.testState, "Failed", "failed");
    elements.testInstruction.textContent = commandErrorMessage(error);
  } finally {
    state.testBusy = false;
    updateDictationControls();
  }
}

async function cancelTest() {
  const sessionId = state.testSessionId;
  if (!invoke || sessionId === null || state.testBusy) return;
  state.testBusy = true;
  stopTestTimer();
  try {
    await invoke("microphone_test_cancel", { sessionId });
    resetTest();
    setPill(elements.testState, "Ready", "idle");
    elements.testInstruction.textContent = "Microphone check cancelled; captured audio was discarded.";
  } catch (error) {
    resetTest();
    setPill(elements.testState, "Failed", "failed");
    elements.testInstruction.textContent = commandErrorMessage(error);
  } finally {
    state.testBusy = false;
    updateDictationControls();
  }
}

async function refreshDiagnostics() {
  if (!invoke) return;
  elements.refreshDiagnostics.disabled = true;
  try {
    const [shortcut, insertion] = await Promise.all([
      invoke("shortcut_capability"),
      invoke("insertion_capability"),
    ]);
    elements.shortcutBackend.textContent = shortcut.selectedBackend || "Unavailable";
    elements.shortcutDetail.textContent = shortcut.registrationError || shortcut.selectionError?.message || shortcut.registrationState;
    elements.insertionBackend.textContent = insertion.backend || "Unavailable";
    elements.insertionDetail.textContent = insertion.available
      ? `${insertion.authorization || "native"} · semantic verification ${insertion.semanticDeliveryVerifiable ? "available" : "not observable"}`
      : insertion.error || "No insertion backend available";
  } catch (error) {
    const message = commandErrorMessage(error);
    elements.shortcutBackend.textContent = "Unavailable";
    elements.shortcutDetail.textContent = message;
    elements.insertionBackend.textContent = "Unavailable";
    elements.insertionDetail.textContent = message;
  } finally {
    elements.refreshDiagnostics.disabled = false;
  }
}

async function recoverDesktopStatus() {
  try {
    const status = await invoke("desktop_status");
    if (status.dictationSessionId && status.dictationState === "recording") {
      state.dictationSessionId = status.dictationSessionId;
      setPill(elements.dictationState, "Recording", "recording");
      elements.dictationMessage.textContent = "A dictation session was already active.";
    } else if (status.dictationState === "idle") {
      setPill(elements.dictationState, "Ready", "idle");
      elements.dictationMessage.textContent = "Press Start dictation or use Ctrl + Shift + Space.";
    }
  } catch (error) {
    showDictationError(error);
  }
}

async function copyTranscript() {
  const text = elements.transcriptText.textContent;
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    elements.copyTranscript.textContent = "Copied";
    window.setTimeout(() => { elements.copyTranscript.textContent = "Copy text"; }, 1200);
  } catch {
    elements.copyTranscript.textContent = "Copy unavailable";
  }
}

async function bootstrap() {
  if (!invoke) {
    setPill(elements.dictationState, "Unavailable", "failed");
    elements.dictationMessage.textContent = "Run this interface inside the BLCVoice desktop application.";
    elements.dictationToggle.disabled = true;
    return;
  }
  try {
    await loadSettings();
    await Promise.all([refreshDevices(), refreshModels(), refreshDiagnostics(), recoverDesktopStatus()]);
  } catch (error) {
    showDictationError(error);
  }
  updateDictationControls();
}

elements.dictationToggle.addEventListener("click", () => void toggleDictation());
elements.dictationCancel.addEventListener("click", () => void cancelDictation());
elements.copyTranscript.addEventListener("click", () => void copyTranscript());
elements.refreshDevices.addEventListener("click", () => void refreshDevices());
elements.deviceSelect.addEventListener("change", () => { renderSelectedDevice(); void saveSelectedDevice(); });
elements.languageSelect.addEventListener("change", () => void saveLanguage());
elements.startTest.addEventListener("click", () => void startTest());
elements.finishTest.addEventListener("click", () => void finishTest(false));
elements.cancelTest.addEventListener("click", () => void cancelTest());
elements.refreshDiagnostics.addEventListener("click", () => void refreshDiagnostics());

void bootstrap();
