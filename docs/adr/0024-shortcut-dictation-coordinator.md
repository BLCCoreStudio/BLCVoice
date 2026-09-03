# ADR 0024: Global shortcut decisions drive dictation through a backend coordinator

## Status

Accepted

## Context

BLCVoice already has three independent production pieces:

- global shortcut registration and normalized `StartDictation` / `StopDictation` decisions,
- configured microphone + local speech-model resolution,
- the capture → ASR → insertion lifecycle.

Keeping the shortcut connected only to a frontend event would make the webview responsible for product-critical orchestration. It would also mean global dictation could stop working when the main window is hidden, not initialized yet, or temporarily busy rendering UI.

There is also an ordering race in toggle-to-talk: model preparation happens before microphone capture starts. A second shortcut press can therefore arrive while the first start is still preparing and before a session ID exists.

## Decision

The Tauri/Rust process owns a `ShortcutDictationCoordinator`.

The coordinator:

- consumes normalized shortcut decisions after platform registration has succeeded,
- starts configured dictation using the same desktop services as explicit IPC commands,
- finishes the real capture → ASR → insertion pipeline on stop,
- keeps only one shortcut-owned lifecycle active,
- queues a stop received during the `Starting` phase and applies it as soon as a recording session exists,
- resets the shortcut controller if starting fails so a failed toggle cannot leave the logical shortcut latch active,
- emits lifecycle events for presentation and diagnostics, but never delegates business logic to the frontend.

Coordinator state is intentionally small:

- `Idle`
- `Starting { stop_requested }`
- `Recording(SessionId)`
- `Finishing(SessionId)`

The existing `blcvoice://shortcut-decision` event remains available as a diagnostic signal. Product UI consumes the separate `blcvoice://dictation-lifecycle` event to mirror backend state.

## Delivery semantics

A coordinator completion means the existing production insertion lifecycle completed. It does not upgrade the insertion backend's semantic guarantees. For example, a platform transport that can prove only that input was submitted still reports that limited guarantee; the coordinator must not call it verified delivery.

If insertion fails, the lifecycle event can include recoverable transcript text so the UI can offer copy/recovery without silently losing recognized speech.

## Consequences

- Global dictation can run without the webview owning the session.
- Shortcut behavior and button-driven dictation use the same configured services and lifecycle semantics.
- The UI becomes a projection of backend state rather than an orchestration dependency.
- Desktop status exposes shortcut coordinator ownership so a reloaded webview can recover state without guessing whether it owns the active session.
- Start/stop races during recognizer preparation are explicit and testable.
- Direct UI sessions and shortcut-owned sessions are still separate control entry points; a future unified command coordinator may consolidate them if cross-control cancellation becomes a product requirement.
