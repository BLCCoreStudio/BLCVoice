# ADR 0014: Microphone test UI and static Tauri bridge

- Status: Accepted
- Date: 2026-09-02

## Context

The desktop host now owns native microphone discovery and bounded capture-test commands, but the existing webview is only a static foundation placeholder. The next usable slice needs to let a person choose an input device and verify that capture succeeds without weakening the native audio boundary or pretending that transcription is already wired into the product.

The current desktop frontend is intentionally build-tool-free HTML/CSS. Introducing React, Vite or another application framework only for this setup screen would add package-management and bundling complexity before the product requires it.

## Decision

The first interactive desktop screen remains a small static HTML/CSS/JavaScript frontend.

Tauri's `app.withGlobalTauri` option is enabled so the static script can call registered custom commands through `window.__TAURI__.core.invoke`. The content-security policy remains self-only for scripts and styles; inline script is not allowed.

The screen may invoke only the existing typed desktop commands required for this slice:

- native input-device discovery,
- desktop session status,
- microphone-test start,
- microphone-test finish,
- microphone-test cancel.

Raw PCM does not cross the IPC boundary. The frontend receives only device metadata, session state and final capture diagnostics. Selected device identity may be remembered in webview-local storage as a convenience, but no audio is persisted there.

The visible test is capped at 10 seconds even though the native collector retains its independent 30-second safety bound. Finishing the test finalizes and validates the capture, reports frame/drop/callback-error diagnostics, and then discards the finalized audio. A successful microphone test must not be represented as successful transcription or dictation.

The UI does not draw a fake microphone level or waveform. A live meter will be added only after the native layer exposes a real bounded measurement contract suitable for UI telemetry.

The desktop CI gate checks the JavaScript for syntax errors and validates the Tauri JSON configuration in addition to the existing Rust desktop tests and Clippy checks.

## Consequences

- The first desktop interaction is useful without committing the project to a frontend framework prematurely.
- The audio privacy boundary stays native: the webview never receives microphone samples.
- Tauri's global JavaScript object is enabled, so CSP and capability permissions remain important security boundaries.
- Physical microphone behavior still cannot be proven by hosted CI. The screen creates a repeatable manual validation path for real Windows, Linux and macOS hardware.
- Model management, ASR execution, VAD, global shortcuts and text insertion remain separate future slices.
