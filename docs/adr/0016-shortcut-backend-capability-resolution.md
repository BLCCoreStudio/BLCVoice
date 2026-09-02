# ADR 0016: Shortcut backend capability resolution

## Status

Accepted.

## Context

ADR 0015 defines the product-level shortcut semantics and the required backend split, but the code previously had no deterministic way to decide which backend was allowed for the current desktop environment.

That gap is especially important on Linux. An application running inside a Wayland session may also expose `DISPLAY` for XWayland clients, so treating the presence of an X11 display as proof that raw X11 global shortcuts are appropriate can select the wrong security model.

BLCVoice needs a small, runtime-independent capability layer before platform registration code is added.

## Decision

`blcvoice-shortcuts` owns desktop shortcut environment facts and backend resolution in addition to interaction semantics.

The resolver uses these backend classes:

- Windows and macOS -> `NativeGlobalHotkey`;
- Linux/X11 -> `X11GlobalHotkey`;
- Linux/Wayland -> `XdgDesktopPortal`;
- unknown Linux display server -> explicit capability error;
- unsupported desktop platform -> explicit capability error.

Linux display-server detection uses the following evidence order:

1. `XDG_SESSION_TYPE` when it explicitly reports `wayland` or `x11`;
2. `WAYLAND_DISPLAY` when the session type is absent or inconclusive;
3. `DISPLAY` as the final X11 signal;
4. otherwise the display server remains unknown.

This ordering intentionally classifies a Wayland session with an XWayland `DISPLAY` socket as Wayland when `XDG_SESSION_TYPE=wayland` or `WAYLAND_DISPLAY` is present.

The capability layer only decides which backend is permitted. It does not register shortcuts itself and therefore does not introduce a Tauri, X11, D-Bus or portal dependency into the runtime-independent crate.

Wayland must never silently fall back to invasive global input capture when the portal path is unavailable.

## Consequences

- Backend selection is deterministic and unit-testable on every CI operating system.
- KDE/GNOME Wayland sessions cannot accidentally be routed to the X11 global-hotkey path merely because XWayland exposes `DISPLAY`.
- Platform adapters can now be implemented behind one explicit resolver instead of repeating environment heuristics.
- Unknown environments fail as a capability problem rather than pretending global shortcuts are available.
- The next implementation step is to bind `NativeGlobalHotkey`/`X11GlobalHotkey` to the native desktop adapter and `XdgDesktopPortal` to a compositor-mediated portal adapter.
