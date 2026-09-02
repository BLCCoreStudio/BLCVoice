# ADR 0015: Global shortcut semantics and backend split

## Status

Accepted.

## Context

BLCVoice must start dictation from outside its own window. The interaction also needs to remain usable for long-form dictation, where holding a key for the entire utterance is unnecessarily fatiguing.

Global shortcut APIs are not uniform across the supported desktop environments. Windows and macOS expose native global-hotkey facilities. Linux/X11 can use traditional global-hotkey registration, while Wayland intentionally restricts arbitrary global input capture and provides compositor-mediated global shortcuts through `org.freedesktop.portal.GlobalShortcuts`.

Native APIs may emit both pressed and released events and may repeat key-down events while a key remains held. Product behavior therefore cannot be inferred directly from raw backend events.

## Decision

The default BLCVoice interaction is **toggle-to-talk** using `Ctrl+Shift+Space`:

1. first distinct press requests dictation start;
2. releasing the shortcut does nothing in toggle mode;
3. the next distinct press requests dictation stop/finalization;
4. keyboard auto-repeat must not toggle the session repeatedly.

Push-to-talk remains an optional interaction mode:

1. distinct press requests dictation start;
2. release requests dictation stop/finalization;
3. repeated pressed events while held are ignored.

The runtime-independent crate `blcvoice-shortcuts` owns these interaction semantics. Platform adapters only translate native activation/deactivation signals into `Pressed`/`Released` phases.

Shortcut state can be forced idle by cancellation or another terminal workflow event. A held push-to-talk key remains physically held after such a reset and therefore cannot immediately restart dictation until it is released and pressed again.

Mode changes are rejected while dictation is requested or while the shortcut is physically held.

Backend policy is capability-driven:

- Windows/macOS: native global shortcut backend;
- Linux/X11: X11-compatible global shortcut backend;
- Linux/Wayland: XDG Desktop Portal `GlobalShortcuts`, using compositor-owned binding and its `Activated`/`Deactivated` signals;
- unsupported or unavailable backends must surface an explicit capability error rather than silently falling back to invasive input capture.

The Wayland path must not bypass compositor security controls.

## Consequences

- Long-form dictation does not require holding the keyboard shortcut.
- Push-to-talk remains available for short utterances.
- Raw native key repeat cannot accidentally create multiple dictation transitions.
- All platform backends share one tested product-level state machine.
- `Ctrl+Shift+Space` is a preferred/default trigger, not an immutable binding; the settings layer may replace it later.
- A future opt-in “dictate and send” shortcut is a separate action and must not change the safety semantics of the normal dictation shortcut.
