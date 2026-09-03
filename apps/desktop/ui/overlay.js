"use strict";

const listen = window.__TAURI__?.event?.listen;
const getCurrentWindow = window.__TAURI__?.window?.getCurrentWindow;
const overlayWindow = getCurrentWindow ? getCurrentWindow() : null;

const title = document.getElementById("overlay-title");
const message = document.getElementById("overlay-message");
const indicator = document.getElementById("state-indicator");
const shortcutHint = document.getElementById("shortcut-hint");

let lifecycleGeneration = 0;
let unlisten = null;

function render(label, detail, kind, showShortcut = true) {
  title.textContent = label;
  message.textContent = detail;
  indicator.className = `state-indicator ${kind}`;
  shortcutHint.hidden = !showShortcut;
}

async function showOverlay() {
  if (!overlayWindow) return;
  try {
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
      render("Preparing", "Loading the selected local model…", "working");
      void showOverlay();
      break;
    case "recording":
      render("Listening", "Press the shortcut again to stop", "recording");
      void showOverlay();
      break;
    case "finishing":
      render("Transcribing", "Processing locally and inserting text…", "working", false);
      void showOverlay();
      break;
    case "completed":
      render("Done", "Transcript submitted to the active app", "success", false);
      void showOverlay();
      void hideOverlay(generation, 1400);
      break;
    case "noSpeech":
      render("No speech", "Nothing was inserted", "idle", false);
      void showOverlay();
      void hideOverlay(generation, 1500);
      break;
    case "failed":
      render("Dictation failed", payload.message || "Open BLCVoice for diagnostics", "failed", false);
      void showOverlay();
      void hideOverlay(generation, 3200);
      break;
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
  if (unlisten) {
    unlisten();
    unlisten = null;
  }
});

void bootstrap();
