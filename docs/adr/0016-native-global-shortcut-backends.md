# ADR 0016: Platform-routed native global shortcut backends

- Status: Accepted
- Date: 2026-09-03

## Context

BLCVoice needs its dictation shortcut to work while another application owns keyboard focus. ADR 0015 defines the product-level toggle-to-talk and push-to-talk semantics independently from any operating-system shortcut API.

The desktop implementations are not equivalent across display environments. The native global-hotkey path used by the Tauri global-shortcut plugin is appropriate for Windows, macOS and Linux/X11, while Linux/Wayland requires the desktop-approved XDG GlobalShortcuts portal rather than an X11 fallback that bypasses compositor policy.

## Decision

- The desktop shell selects a shortcut backend by runtime platform/display capability.
- Windows and macOS use `tauri-plugin-global-shortcut` 2.3.2.
- Linux/X11 uses the same native global-hotkey adapter.
- Linux/Wayland uses the XDG Desktop Portal `GlobalShortcuts` interface through `ashpd` 0.13.13.
- Detecting a Wayland session must select the portal path; BLCVoice does not silently fall back to XWayland shortcut capture.
- `Ctrl+Shift+Space` is the initial preferred trigger. On Wayland, the portal/compositor may present or assign another binding; the trigger returned by the portal is treated as authoritative for diagnostics/UI.
- Native press/release or portal activated/deactivated events feed the shared `blcvoice-shortcuts::ShortcutController`. Backends do not duplicate toggle or push-to-talk policy.
- Backends emit typed desktop `startDictation` / `stopDictation` decisions. They do not own microphone capture, ASR, text insertion or target application behavior.
- Shortcut registration failures are non-fatal to application startup. The desktop exposes backend, registration state and actionable error information instead.
- Shortcut mode changes continue to use the shared controller and are rejected while the shortcut/session is active.

## Consequences

- Wayland may display a portal-controlled shortcut binding prompt and the desktop environment retains authority over the final binding.
- A shortcut registration failure leaves microphone setup and other application functionality usable rather than terminating BLCVoice.
- Windows/macOS/X11 and Wayland share one interaction state machine despite using different native transports.
- CI can prove the adapters compile and their pure routing/state logic passes tests, but it cannot prove compositor registration or physical keyboard events. KDE/Wayland, X11, Windows and macOS runtime validation remains required.
- Connecting shortcut decisions to the production capture → ASR → insertion lifecycle is a separate application-orchestration decision.
