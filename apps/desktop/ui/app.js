"use strict";

const AUTO_FINISH_MS = 10_000;
const SELECTED_DEVICE_KEY = "blcvoice.selectedInputDevice";

const invoke = window.__TAURI__?.core?.invoke;

const elements = {
  deviceSelect: document.getElementById("device-select"),
  refreshDevices: document.getElementById("refresh-devices"),
  deviceBackend: document.getElementById("device-backend"),
  deviceFormat: document.getElementById("device-format"),
  deviceDefault: document.getElementById("device-default"),
  discoveryMessage: document.getElementById("discovery-message"),
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
  errorMessage: document.getElementById("error-message"),
};

const state = {
  devices: [],
  selectedBackend: null,
  activeSessionId: null,
  startedAt: null,
  timerId: null,
  busy: false,
  autoFinishStarted: false,
};

function selectedDevice() {
  return state.devices.find((device) => device.id === elements.deviceSelect.value) ?? null;
}

function persistedDeviceId() {
  try {
    return window.localStorage.getItem(SELECTED_DEVICE_KEY);
  } catch {
    return null;
  }
}

function persistDeviceId(deviceId) {
  try {
    window.localStorage.setItem(SELECTED_DEVICE_KEY, deviceId);
  } catch {
    // Device persistence is optional; capture must continue to work without storage access.
  }
}

function commandErrorMessage(error) {
  if (error && typeof error === "object") {
    const code = typeof error.code === "string" ? error.code : null;
    const message = typeof error.message === "string" ? error.message : null;
    if (message && code) {
      return `${message} (${code})`;
    }
    if (message) {
      return message;
    }
  }
  if (typeof error === "string") {
    return error;
  }
  return "The desktop bridge returned an unknown error.";
}

function setError(message) {
  elements.errorMessage.textContent = message;
  elements.errorMessage.hidden = !message;
}

function clearResult() {
  elements.resultCard.hidden = true;
  elements.resultSummary.textContent = "";
  elements.resultFrames.textContent = "—";
  elements.resultDropped.textContent = "—";
  elements.resultErrors.textContent = "—";
}

function setStatePill(label, kind) {
  elements.testState.textContent = label;
  elements.testState.className = `state-pill ${kind}`;
}

function formatNativeConfig(config) {
  if (!config) {
    return "Unavailable";
  }
  const channels = config.channels === 1 ? "mono" : `${config.channels} ch`;
  const rateKhz = (config.sampleRateHz / 1000).toLocaleString(undefined, {
    maximumFractionDigits: 1,
  });
  return `${rateKhz} kHz · ${channels} · ${config.sampleFormat}`;
}

function renderSelectedDevice() {
  const device = selectedDevice();
  if (!device) {
    elements.deviceBackend.textContent = "—";
    elements.deviceFormat.textContent = "—";
    elements.deviceDefault.textContent = "—";
    updateControls();
    return;
  }

  elements.deviceBackend.textContent = device.backend || state.selectedBackend || "Unknown";
  elements.deviceFormat.textContent = formatNativeConfig(device.defaultConfig);
  elements.deviceDefault.textContent = device.isDefault ? "Yes" : "No";
  persistDeviceId(device.id);
  updateControls();
}

function chooseInitialDevice() {
  const persisted = persistedDeviceId();
  const choice =
    state.devices.find((device) => device.id === persisted) ??
    state.devices.find((device) => device.isDefault) ??
    state.devices[0] ??
    null;

  if (choice) {
    elements.deviceSelect.value = choice.id;
  }
  renderSelectedDevice();
}

function renderDiscovery(discovery) {
  state.devices = Array.isArray(discovery.devices) ? discovery.devices : [];
  state.selectedBackend = discovery.selectedBackend ?? null;
  elements.deviceSelect.replaceChildren();

  if (state.devices.length === 0) {
    const option = document.createElement("option");
    option.textContent = "No usable input devices found";
    option.value = "";
    elements.deviceSelect.append(option);
  } else {
    for (const device of state.devices) {
      const option = document.createElement("option");
      option.value = device.id;
      option.textContent = device.isDefault ? `${device.name} — Default` : device.name;
      elements.deviceSelect.append(option);
    }
  }

  const failures = Array.isArray(discovery.failures) ? discovery.failures : [];
  if (failures.length > 0) {
    const first = failures[0];
    const suffix = failures.length > 1 ? ` +${failures.length - 1} more` : "";
    elements.discoveryMessage.textContent = `Discovery warning: ${first.message}${suffix}`;
  } else if (state.devices.length > 0) {
    elements.discoveryMessage.textContent = `${state.devices.length} input device${state.devices.length === 1 ? "" : "s"} available.`;
  } else {
    elements.discoveryMessage.textContent = "No microphone is currently available to BLCVoice.";
  }

  chooseInitialDevice();
}

async function refreshDevices() {
  if (!invoke || state.busy || state.activeSessionId !== null) {
    return;
  }

  setError("");
  state.busy = true;
  elements.discoveryMessage.textContent = "Discovering native input devices…";
  updateControls();

  try {
    const discovery = await invoke("audio_input_discovery");
    renderDiscovery(discovery);
  } catch (error) {
    state.devices = [];
    elements.deviceSelect.replaceChildren();
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "Discovery failed";
    elements.deviceSelect.append(option);
    renderSelectedDevice();
    setError(commandErrorMessage(error));
  } finally {
    state.busy = false;
    updateControls();
  }
}

function stopTimer() {
  if (state.timerId !== null) {
    window.clearInterval(state.timerId);
    state.timerId = null;
  }
}

function renderTimer() {
  if (state.startedAt === null) {
    elements.testTimer.textContent = "0.0s";
    elements.testProgress.value = 0;
    return;
  }

  const elapsedMs = Math.max(0, Date.now() - state.startedAt);
  const seconds = Math.min(AUTO_FINISH_MS, elapsedMs) / 1000;
  elements.testTimer.textContent = `${seconds.toFixed(1)}s`;
  elements.testProgress.value = seconds;

  if (
    elapsedMs >= AUTO_FINISH_MS &&
    state.activeSessionId !== null &&
    !state.busy &&
    !state.autoFinishStarted
  ) {
    state.autoFinishStarted = true;
    void finishTest(true);
  }
}

function startTimer() {
  stopTimer();
  state.startedAt = Date.now();
  state.autoFinishStarted = false;
  renderTimer();
  state.timerId = window.setInterval(renderTimer, 100);
}

function resetActiveSession() {
  stopTimer();
  state.activeSessionId = null;
  state.startedAt = null;
  state.autoFinishStarted = false;
  elements.testProgressWrap.hidden = true;
  elements.testProgress.value = 0;
  elements.testTimer.textContent = "0.0s";
}

function renderSuccess(report, automatic) {
  const stats = report.captureStats;
  elements.resultTitle.textContent = "Capture validated";
  elements.resultBadge.textContent = "Passed";
  elements.resultBadge.className = "result-badge success";
  elements.resultFrames.textContent = Number(report.capturedFrames).toLocaleString();
  elements.resultDropped.textContent = Number(stats.droppedSamples).toLocaleString();
  elements.resultErrors.textContent = Number(stats.callbackErrors).toLocaleString();
  elements.resultSummary.textContent = automatic
    ? "The 10-second test completed cleanly. Captured audio was discarded after validation."
    : "The native capture stream finalized cleanly. Captured audio was discarded after validation.";
  elements.resultCard.hidden = false;
}

function updateControls() {
  const hasDevice = selectedDevice() !== null;
  const active = state.activeSessionId !== null;

  elements.deviceSelect.disabled = state.busy || active || state.devices.length === 0;
  elements.refreshDevices.disabled = state.busy || active;
  elements.startTest.disabled = state.busy || active || !hasDevice || !invoke;
  elements.startTest.hidden = active;
  elements.finishTest.hidden = !active;
  elements.finishTest.disabled = state.busy;
  elements.cancelTest.hidden = !active;
  elements.cancelTest.disabled = state.busy;
}

async function startTest() {
  const device = selectedDevice();
  if (!invoke || !device || state.busy || state.activeSessionId !== null) {
    return;
  }

  setError("");
  clearResult();
  state.busy = true;
  setStatePill("Starting", "working");
  elements.testInstruction.textContent = "Opening the native input stream…";
  updateControls();

  try {
    const session = await invoke("microphone_test_start", { deviceId: device.id });
    state.activeSessionId = session.id;
    elements.testProgressWrap.hidden = false;
    setStatePill("Recording", "recording");
    elements.testInstruction.textContent = "Speak normally. You can finish early, or BLCVoice will stop after 10 seconds.";
    startTimer();
  } catch (error) {
    setStatePill("Failed", "failed");
    elements.testInstruction.textContent = "The microphone test could not start.";
    setError(commandErrorMessage(error));
  } finally {
    state.busy = false;
    updateControls();
  }
}

async function finishTest(automatic = false) {
  const sessionId = state.activeSessionId;
  if (!invoke || sessionId === null || state.busy) {
    return;
  }

  state.busy = true;
  stopTimer();
  renderTimer();
  setStatePill("Validating", "working");
  elements.testInstruction.textContent = "Finalizing the native stream and checking capture integrity…";
  updateControls();

  try {
    const report = await invoke("microphone_test_finish", { sessionId });
    resetActiveSession();
    renderSuccess(report, automatic);
    setStatePill("Passed", "passed");
    elements.testInstruction.textContent = "This microphone is ready for the next BLCVoice development stage.";
    setError("");
  } catch (error) {
    resetActiveSession();
    setStatePill("Failed", "failed");
    elements.testInstruction.textContent = "Capture did not pass the native integrity check.";
    setError(commandErrorMessage(error));
  } finally {
    state.busy = false;
    updateControls();
  }
}

async function cancelTest() {
  const sessionId = state.activeSessionId;
  if (!invoke || sessionId === null || state.busy) {
    return;
  }

  state.busy = true;
  stopTimer();
  setStatePill("Cancelling", "working");
  updateControls();

  try {
    await invoke("microphone_test_cancel", { sessionId });
    resetActiveSession();
    setStatePill("Cancelled", "idle");
    elements.testInstruction.textContent = "The microphone test was cancelled and its captured audio was discarded.";
    setError("");
  } catch (error) {
    resetActiveSession();
    setStatePill("Failed", "failed");
    elements.testInstruction.textContent = "The native capture session could not be cancelled cleanly.";
    setError(commandErrorMessage(error));
  } finally {
    state.busy = false;
    updateControls();
  }
}

async function recoverDesktopStatus() {
  if (!invoke) {
    return;
  }

  try {
    const status = await invoke("desktop_status");
    if (status.lastPumpFailure) {
      setError(`Previous capture worker failure: ${status.lastPumpFailure}`);
    }

    if (status.session?.state === "recording") {
      state.activeSessionId = status.session.id;
      elements.testProgressWrap.hidden = false;
      setStatePill("Recording", "recording");
      elements.testInstruction.textContent = "A native microphone test was already active. You can finish or cancel it here.";
      startTimer();
    }
  } catch (error) {
    setError(commandErrorMessage(error));
  }
}

async function bootstrap() {
  if (!invoke) {
    setStatePill("Unavailable", "failed");
    elements.discoveryMessage.textContent = "The Tauri desktop bridge is unavailable.";
    setError("Run this screen inside the BLCVoice desktop application; it is not a standalone web page.");
    updateControls();
    return;
  }

  await recoverDesktopStatus();
  if (state.activeSessionId === null) {
    await refreshDevices();
  } else {
    updateControls();
  }
}

elements.deviceSelect.addEventListener("change", renderSelectedDevice);
elements.refreshDevices.addEventListener("click", () => void refreshDevices());
elements.startTest.addEventListener("click", () => void startTest());
elements.finishTest.addEventListener("click", () => void finishTest(false));
elements.cancelTest.addEventListener("click", () => void cancelTest());

void bootstrap();
