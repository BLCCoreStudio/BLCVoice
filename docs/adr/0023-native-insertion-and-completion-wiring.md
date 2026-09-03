# ADR 0023: Native insertion and completion wiring

## Status

Accepted

## Context

The production desktop dictation service already captured audio, ran local ASR and stopped truthfully in `Inserting`. Linux/Wayland and Linux/X11 had concrete insertion adapters, but Windows/macOS did not and no adapter was connected to the production lifecycle.

A backend accepting synthetic input is not proof that the focused application's document changed. The lifecycle therefore needs to distinguish successful protocol submission from semantic delivery verification while still allowing a completed user operation to release the dictation slot.

## Decision

- Windows and macOS use a dedicated `blcvoice-insertion-native` adapter backed by Enigo's native platform implementation.
- Windows reports the `WindowsSendInput` capability. It remains subject to Windows integrity/UIPI restrictions.
- macOS reports the `MacOsQuartz` capability and requests Accessibility authorization when needed.
- `Keyboard::text` is used for layout-independent Unicode text submission on Windows/macOS.
- Linux/X11 continues to use the BLCVoice XTEST adapter.
- Linux/Wayland continues to use the XDG RemoteDesktop + EIS `ei_text` adapter and never falls back to raw input devices or root helpers.
- The desktop insertion service resolves one backend from the detected environment and connects lazily, so Wayland authorization is not prompted at application startup.
- The dictation slot gains an explicit `Inserting` ownership state so insertion cannot race with cancellation or another session.
- After ASR, production `dictation_finish` claims insertion, submits the transcript, then commits `InsertionDelivered` only when the selected backend accepted the full text.
- On insertion failure, the runtime records `FailureStage::TextInsertion` and the IPC error includes the transcript as `recoverableText` so user data is not lost.
- A successful receipt continues to report `semanticDeliveryVerified = false`; BLCVoice does not claim that arbitrary target application state is observable.

## Consequences

The desktop command path now has a complete capture -> ASR -> insertion -> terminal lifecycle on all four capability environments: Windows, macOS, Linux/X11 and Linux/Wayland. Real desktop compatibility still requires platform-specific runtime validation, especially across Wayland compositors and elevated Windows targets.
