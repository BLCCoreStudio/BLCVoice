# ADR 0016: Desktop global shortcut backends

## Status

Accepted.

## Context

BLCVoice needs a global dictation shortcut while its window is unfocused or hidden. The product-level toggle/push-to-talk semantics are already isolated in `blcvoice-shortcuts`, but operating systems expose different registration mechanisms.

Traditional desktop global-hotkey APIs are appropriate on Windows, macOS and Linux/X11. Wayland deliberately prevents applications from globally intercepting arbitrary keyboard input. Its supported path is compositor-mediated registration through the XDG Desktop Portal `org.freedesktop.portal.GlobalShortcuts` interface.

## Decision

BLCVoice selects the shortcut backend by capability/session:

- Windows and macOS use the Tauri global-shortcut plugin from Rust;
- Linux/X11 uses the same native/global-hotkey adapter;
- Linux/Wayland uses the XDG Desktop Portal GlobalShortcuts interface through `ashpd`;
- a non-empty `WAYLAND_DISPLAY` or `XDG_SESSION_TYPE=wayland` selects the portal path, even if X11 compatibility variables are also present.

The initial preferred trigger remains `Ctrl+Shift+Space`.

On Wayland, the portal/compositor owns the binding. BLCVoice submits `CTRL+SHIFT+space` as a preferred trigger, then records the compositor-returned human-readable `trigger_description` as the effective binding. BLCVoice does not use input-capture hacks to bypass compositor policy.

Both native and portal backends translate only activation/deactivation events into the runtime-independent `ShortcutPhase` contract. Product behavior remains in `blcvoice-shortcuts`.

Registration failures are non-fatal to application startup. They are retained as structured desktop diagnostics with backend, registration state and the last error. Activation/deactivation counts and the latest product-level shortcut decision are also exposed for physical validation.

This change intentionally stops at the shortcut event boundary. It does not start microphone capture yet. Background dictation requires microphone and shortcut preferences to live in native application state rather than only in the webview's local storage; that is a separate follow-up boundary.

## Consequences

- KDE/GNOME Wayland can use the compositor-supported global-shortcut path.
- Windows/macOS/X11 do not depend on a portal service.
- The application can still open if the preferred shortcut is unavailable or already owned by another application.
- CI can compile the desktop integration on Windows/macOS and compile/test/lint the Linux desktop adapter, but actual OS registration and portal interaction still require physical-session validation.
- No raw global keyboard monitoring is introduced.
