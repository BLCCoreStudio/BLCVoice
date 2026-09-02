# ADR 0018: Platform global-shortcut registration

## Status

Accepted.

## Context

BLCVoice already owns runtime-independent toggle-to-talk and push-to-talk semantics, and it can resolve the permitted shortcut backend for Windows, macOS, Linux/X11 and Linux/Wayland. The desktop application now needs to perform actual operating-system/compositor registration without collapsing backend selection, registration success and dictation execution into one state.

Linux/Wayland cannot use the X11 global-hotkey mechanism safely. The supported native global-hotkey implementation is appropriate for Windows, macOS and Linux/X11, while Wayland provides compositor-mediated registration through `org.freedesktop.portal.GlobalShortcuts`.

The current desktop capture service is still a microphone-test boundary rather than a complete production dictation service. Shortcut registration therefore must not call microphone-test operations simply to make the end-to-end interaction appear finished.

## Decision

The desktop host owns a `ShortcutService` that combines:

- the capability-selected backend;
- explicit registration lifecycle state;
- typed registration failure text for diagnostics;
- the runtime-independent `ShortcutController`.

Registration states are `pending`, `registering`, `registered`, `failed` and `unavailable`.

Windows, macOS and Linux/X11 use `tauri-plugin-global-shortcut`. The plugin is initialized only when the shared capability resolver selects the native/X11 backend, so a Wayland session never attempts the underlying X11-only global-hotkey implementation.

Linux/Wayland uses `ashpd` and the XDG `GlobalShortcuts` portal. BLCVoice creates a portal session, requests the stable application shortcut ID `dictation.toggle`, supplies `CTRL+SHIFT+space` as the preferred trigger, verifies that the binding response contains the BLCVoice shortcut, and listens for `Activated` and `Deactivated` signals for the lifetime of the session.

Both backend families translate their events into the shared `Pressed`/`Released` phases. Only a successfully registered service may feed those phases into `ShortcutController`.

Non-ignored controller decisions are emitted from the desktop host as the typed event `blcvoice://shortcut-decision`, with `startDictation` or `stopDictation`. This event is an application-orchestration boundary, not proof that recording or transcription has started.

Registration failure does not terminate the desktop application. The service records a `failed` state and actionable error instead.

## Consequences

- `Ctrl+Shift+Space` can be registered through the correct platform security model.
- Wayland never silently falls back to X11-style global input capture.
- Portal cancellation or compositor rejection remains distinguishable from backend selection success.
- Toggle/push-to-talk semantics remain identical across backend implementations.
- Shortcut registration can be tested independently from microphone/model/ASR readiness.
- The next product step is a real desktop dictation orchestration service that consumes `startDictation`/`stopDictation` decisions and owns selected microphone, model, recognizer, finalization and later insertion lifecycle.
