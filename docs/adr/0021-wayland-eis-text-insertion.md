# ADR 0021: Wayland text insertion via XDG RemoteDesktop and EIS

## Status

Accepted

## Context

BLCVoice needs to place recognized UTF-8 text into the application that currently owns keyboard focus. Wayland intentionally prevents ordinary clients from globally synthesizing input, so the X11-style approach cannot be treated as a generic Wayland fallback.

The text-insertion capability boundary already selects `XdgRemoteDesktopEis` for Linux/Wayland. The concrete adapter must preserve that boundary, request no unrelated capabilities, remain compatible with the workspace's `unsafe_code = "forbid"` policy and avoid claiming that successful protocol submission proves semantic mutation of the target application.

## Decision

The initial Wayland adapter uses the XDG Desktop Portal `RemoteDesktop` interface and its `ConnectToEIS` path.

- Request only the portal `Keyboard` device capability. Pointer and touchscreen control are not requested.
- Use a dedicated worker thread with its own Tokio runtime to own the portal session and continuously service the EIS event stream.
- Negotiate an EIS sender connection through `reis` and require a resumed device exposing `ei_text`.
- Bind only keyboard/text EIS capabilities needed by dictation.
- Prefer `ei_text.utf8` rather than synthesizing layout-dependent keycodes.
- Split text on UTF-8 character boundaries into chunks of at most 254 bytes, matching the `ei_text.utf8` protocol payload limit.
- Submit at most one UTF-8 request per EIS device frame and use `CLOCK_MONOTONIC` microseconds for frame timestamps.
- Flush each submitted chunk so a later transport failure can be reported as `PartialSubmission` rather than as an all-or-nothing success.
- Reject empty strings and embedded NUL bytes before backend submission.
- Treat portal denial/cancellation as `PermissionDenied` and missing/disconnected `ei_text` capability as `BackendUnavailable`.
- Support portal restore tokens so later configuration persistence can reduce repeated consent prompts where the compositor/portal permits it.
- Do not fall back from Wayland to X11, evdev, uinput, root access or another privilege bypass.
- A complete `InsertionReceipt` means the EIS transport accepted the full payload for submission. It does not prove that the focused application mutated its text content.

## Consequences

The Wayland implementation respects compositor mediation and asks for less privilege than generic remote-control implementations that also request pointer access. Direct UTF-8 submission avoids keyboard-layout reconstruction for normal dictation text.

Compatibility depends on the active portal/compositor exposing a usable EIS text device. Systems that provide keyboard emulation but not `ei_text` are reported as unavailable in this first adapter instead of silently switching to a less predictable path.

The adapter is deliberately independent from the desktop dictation service for now. Wiring successful ASR output into insertion, persisting restore tokens and exercising real KDE/GNOME runtime behavior are separate follow-up changes.
