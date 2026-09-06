"use strict";

const listen = window.__TAURI__?.event?.listen;
const tauriWindow = window.__TAURI__?.window;
const getCurrentWindow = tauriWindow?.getCurrentWindow;
const overlayWindow = getCurrentWindow ? getCurrentWindow() : null;

const shell = document.getElementById("overlay-shell");
const title = document.getElementById("overlay-title");
const message = document.getElementById("overlay-message");
const shortcutHint = document.getElementById("shortcut-hint");
const recordingTimer = document.getElementById("recording-timer");

let lifecycleGeneration = 0;
let unlisten = null;
let timerId = null;
let recordingStartedAt = null;

function truncate(text, max = 78) {
  if (typeof text !== "string") return "";
  const normalized = text.replace(/\s+/g, " ").trim();
  return normalized.length > max ? `${normalized.slice(0, max - 1)}…` : normalized;
}

function formatElapsed(ms) {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function stopTimer() {
  if (timerId !== null) {
    window.clearInterval(timerId);
    timerId = null;
  }
  recordingStartedAt = null;
  recordingTimer.hidden = true;
}

function startTimer() {
  stopTimer();
  recordingStartedAt = Date.now();
  recordingTimer.textContent = "00:00";
  recordingTimer.hidden = false;
  timerId = window.setInterval(() => {
    if (recordingStartedAt === null) return;
    recordingTimer.textContent = formatElapsed(Date.now() - recordingStartedAt);
  }, 250);
}

function render(label, detail, kind, options = {}) {
  const { showShortcut = true, showTimer = false } = options;
  title.textContent = label;
  message.textContent = detail;
  shell.className = `overlay-shell ${kind}`;
  shortcutHint.hidden = !showShortcut;
  if (!showTimer) recordingTimer.hidden = true;
}

async function positionNearLeftEdge() {
  if (!overlayWindow || !tauriWindow?.currentMonitor || !tauriWindow?.LogicalPosition) return;
  try {
    const monitor = await tauriWindow.currentMonitor();
    if (!monitor) return;

    const scale = monitor.scaleFactor || 1;
    const logicalX = monitor.position.x / scale + 22;
    const logicalHeight = monitor.size.height / scale;
    const logicalY = monitor.position.y / scale + Math.max(22, (logicalHeight - 88) / 2);
    await overlayWindow.setPosition(
      new tauriWindow.LogicalPosition(Math.round(logicalX), Math.round(logicalY)),
    );
  } catch {
    // Positioning is cosmetic. Never let it interfere with dictation.
  }
}

async function showOverlay() {
  if (!overlayWindow) return;
  try {
    await positionNearLeftEdge();
    await overlayWindow.show();
  } catch {
    // The overlay is advisory UI; a visibility failure must never affect dictation.
  }
}

async function hideOverlay(generation, delayMs) {
  await new Promise((resolve) => window.setTimeout(resolve, delayMs));
  if (generation !== lifecycleGeneration || !overlayWindow) return;
  try {
    await overlayWindow.hide();
  } catch {
    // Dictation lifecycle remains authoritative even if the overlay cannot hide itself.
  }
}

function applyLifecycle(payload) {
  if (!payload || payload.source !== "shortcut") return;
  lifecycleGeneration += 1;
  const generation = lifecycleGeneration;

  switch (payload.state) {
    case "starting":
      stopTimer();
      render("Preparing", "Loading the selected local model…", "working", { showShortcut: false });
      void showOverlay();
      break;
    case "recording":
      startTimer();
      render("Listening", "Speak naturally · press the shortcut again to stop", "recording", {
        showShortcut: true,
        showTimer: true,
      });
      recordingTimer.hidden = false;
      void showOverlay();
      break;
    case "finishing":
      stopTimer();
      render("Transcribing", "Processing your speech locally…", "working", { showShortcut: false });
      void showOverlay();
      break;
    case "completed": {
      stopTimer();
      const preview = truncate(payload.text);
      render("Sent ✓", preview || "Transcript submitted to the focused app", "success", {
        showShortcut: false,
      });
      void showOverlay();
      void hideOverlay(generation, 1700);
      break;
    }
    case "noSpeech":
      stopTimer();
      render("No speech", "Nothing was inserted", "idle", { showShortcut: false });
      void showOverlay();
      void hideOverlay(generation, 1500);
      break;
    case "failed": {
      stopTimer();
      const recovered = truncate(payload.recoverableText);
      const detail = recovered
        ? `Text recovered: ${recovered}`
        : payload.message || "Open BLCVoice for diagnostics";
      render("Not sent", detail, "failed", { showShortcut: false });
      void showOverlay();
      void hideOverlay(generation, recovered ? 4200 : 3200);
      break;
    }
    default:
      break;
  }
}

async function bootstrap() {
  if (!listen || !overlayWindow) return;
  try {
    unlisten = await listen("blcvoice://dictation-lifecycle", (event) => {
      applyLifecycle(event.payload);
    });
  } catch {
    unlisten = null;
  }
}

window.addEventListener("beforeunload", () => {
  stopTimer();
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
});

void bootstrap();
