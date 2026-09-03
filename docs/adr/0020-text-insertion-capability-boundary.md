# ADR 0020: Text insertion capability boundary

- Status: Accepted
- Date: 2026-09-03

## Context

BLCVoice reaches a real transcription but intentionally stops in the domain `Inserting` state because no production text-delivery adapter exists yet. Text insertion is not one portable primitive: Windows, macOS, Linux/X11 and Linux/Wayland expose different security models, permissions and acknowledgement semantics.

A platform API accepting synthetic input also does not prove that the focused application's editable document changed. Treating API submission as semantic delivery would repeat the exact reliability mistake the domain lifecycle is designed to avoid.

## Decision

BLCVoice introduces two runtime-independent crates before implementing native adapters:

1. `blcvoice-platform` owns shared desktop environment facts and Linux display-server detection.
2. `blcvoice-insertion` owns backend capability resolution and the text-inserter contract.

The insertion resolver selects:

- Windows: native `SendInput` path.
- macOS: Quartz/CGEvent path, with Accessibility authorization represented explicitly.
- Linux/X11: XTEST-based path.
- Linux/Wayland: compositor-mediated XDG RemoteDesktop + EIS path, with portal authorization represented explicitly.

Unknown Linux display-server state is a typed capability error. Wayland never silently falls back to X11, evdev, uinput, root-only injection or another privileged global-input workaround.

`TextInserter::insert_text` may return `Ok(InsertionReceipt)` only when the selected backend reports complete submission of the supplied text according to that backend's acknowledgement rules. Partial native submissions are errors.

An `InsertionReceipt` explicitly does **not** claim that the target application's content was semantically verified. Synthetic input APIs generally do not expose such verification.

## Consequences

- Shortcut and insertion subsystems share one source of truth for desktop platform/display-server facts.
- Platform adapters can be implemented and tested independently without changing domain semantics.
- Permission readiness becomes visible rather than an implicit failure discovered during delivery.
- Wayland remains compatible with its compositor security model.
- The dictation runtime must remain in `Inserting` until a concrete adapter returns a complete submission receipt and the orchestration layer records delivery.
- Clipboard-based fallback, if later justified, must be designed explicitly; it is not part of this capability contract.

## Not in this decision

This ADR does not implement `SendInput`, Quartz events, XTEST, RemoteDesktop/EIS, clipboard fallback, focused-window discovery, semantic document verification or shortcut-to-dictation wiring.
