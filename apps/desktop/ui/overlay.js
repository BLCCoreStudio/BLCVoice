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

const OVERLAY_WIDTH = 268;
const OVERLAY_HEIGHT = 52;
const POSITION_STORAGE_KEY = "blcvoice.overlay.position.v1";

let lifecycleGeneration = 0;
let unlisten = null;
let unlistenMoved = null;
let timerId = null;
let recordingStartedAt = null;
let positioned = false;

function truncate(text, max = 28) {
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
  const { showShortcut = false, showTimer = false } = options;
  title.textContent = label;
  message.textContent = detail || "";
  message.hidden = !detail;
  shell.className = `overlay-shell ${kind}`;
  shortcutHint.hidden = !showShortcut;
  if (!showTimer) recordingTimer.hidden = true;
}

async function ensureCompactSize() {
  if (!overlayWindow || !tauriWindow?.LogicalSize) return;
  try {
    await overlayWindow.setSize(new tauriWindow.LogicalSize(OVERLAY_WIDTH, OVERLAY_HEIGHT));
  } catch {
    // The static Tauri config has the same size; this is a runtime safety net.
  }
}

function loadSavedPosition() {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(POSITION_STORAGE_KEY) || "null");
    if (!parsed || !Number.isFinite(parsed.x) || !Number.isFinite(parsed.y)) return null;
    return parsed;
  } catch {
    return null;
  }
}

async function positionInitially() {
  if (positioned || !overlayWindow) return;

  const saved = loadSavedPosition();
  if (saved && tauriWindow?.PhysicalPosition) {
    try {
      await overlayWindow.setPosition(
        new tauriWindow.PhysicalPosition(Math.round(saved.x), Math.round(saved.y)),
      );
      positioned = true;
      return;
    } catch {
      // Wayland intentionally does not expose global window coordinates.
    }
  }

  if (!tauriWindow?.currentMonitor || !tauriWindow?.LogicalPosition) return;
  try {
    const monitor = await tauriWindow.currentMonitor();
    if (!monitor) return;

    const scale = monitor.scaleFactor || 1;
    const logicalX = monitor.position.x / scale + 18;
    const logicalHeight = monitor.size.height / scale;
    const logicalY = monitor.position.y / scale + Math.max(18, (logicalHeight - OVERLAY_HEIGHT) * 0.45);
    await overlayWindow.setPosition(
      new tauriWindow.LogicalPosition(Math.round(logicalX), Math.round(logicalY)),
    );
    positioned = true;
  } catch {
    // On Wayland the compositor owns absolute placement; drag remains available.
  }
}

async function showOverlay() {
  if (!overlayWindow) return;
  try {
    await ensureCompactSize();
    await positionInitially();
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
      render("Preparing", "Loading model", "working");
      void showOverlay();
      break;
    case "recording":
      startTimer();
      render("Listening", "Speak", "recording", {
        showShortcut: true,
        showTimer: true,
      });
      recordingTimer.hidden = false;
      void showOverlay();
      break;
    case "finishing":
      stopTimer();
      render("Transcribing", "Local", "working");
      void showOverlay();
      break;
    case "completed": {
      stopTimer();
      const preview = truncate(payload.text);
      render("Sent ✓", preview, "success");
      void showOverlay();
      void hideOverlay(generation, 1250);
      break;
    }
    case "noSpeech":
      stopTimer();
      render("No speech", "Nothing sent", "idle");
      void showOverlay();
      void hideOverlay(generation, 1200);
      break;
    case "failed":
      stopTimer();
      render(
        "Not sent",
        payload.recoverableText ? "Text recovered" : "Open BLCVoice",
        "failed",
      );
      void showOverlay();
      void hideOverlay(generation, 2600);
      break;
    default:
      break;
  }
}

async function watchPosition() {
  if (!overlayWindow?.onMoved) return;
  try {
    unlistenMoved = await overlayWindow.onMoved(({ payload }) => {
      if (!payload || !Number.isFinite(payload.x) || !Number.isFinite(payload.y)) return;
      window.localStorage.setItem(
        POSITION_STORAGE_KEY,
        JSON.stringify({ x: payload.x, y: payload.y }),
      );
      positioned = true;
    });
  } catch {
    unlistenMoved = null;
  }
}

function installDragBehavior() {
  if (!shell || !overlayWindow?.startDragging) return;
  shell.addEventListener("mousedown", (event) => {
    if (event.button !== 0) return;
    void overlayWindow.startDragging().catch(() => {});
  });
}

async function bootstrap() {
  if (!listen || !overlayWindow) return;
  await ensureCompactSize();
  await watchPosition();
  installDragBehavior();
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
  if (unlistenMoved) {
    unlistenMoved();
    unlistenMoved = null;
  }
});

void bootstrap();
