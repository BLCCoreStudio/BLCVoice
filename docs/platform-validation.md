# Real-platform compatibility validation

This document is the canonical runtime-evidence matrix for BLCVoice desktop compatibility. Compile, lint, unit-test and package success are necessary but do not prove end-to-end desktop behavior.

## Status vocabulary

Each environment row must use exactly one status:

- `UNVALIDATED`: no representative runtime evidence yet.
- `PARTIAL`: some critical-path stages passed, but the full row is not proven.
- `VALIDATED`: every required critical-path scenario passed on the recorded environment and commit.
- `UNSUPPORTED`: the implementation intentionally does not support this environment or capability.
- `BLOCKED_EXTERNAL`: validation requires unavailable physical/session/account access rather than a repository change.

Never promote a generic platform support claim from compile/package evidence alone.

## Critical dictation path

Every `VALIDATED` row must exercise the same application path:

`global shortcut -> microphone capture -> VAD/endpointer -> local ASR -> target/insertion capability resolution -> text submission -> observable outcome`

The evidence must distinguish **backend/protocol acceptance** from **semantic target-document verification**. A backend returning success is not enough: the submitted text must be observed in the intended target when the platform permits that observation.

## Minimum matrix

| Environment | Required insertion path | Current status |
| --- | --- | --- |
| Windows current supported desktop | native Windows insertion | UNVALIDATED |
| macOS current supported desktop | native macOS insertion | UNVALIDATED |
| Linux/X11 | XTEST insertion | PARTIAL — live Xvfb/XTEST smoke exists; full dictation session still unvalidated |
| KDE Plasma 6 Wayland + `xdg-desktop-portal-kde` | RemoteDesktop portal + EIS | UNVALIDATED |
| GNOME Wayland + active portal backend | RemoteDesktop portal + EIS | UNVALIDATED |

## Evidence record

For each tested row, save all of the following together:

- exact BLCVoice commit SHA and package/build provenance;
- OS version and architecture;
- desktop environment version;
- compositor/session type and version where applicable;
- portal frontend/backend versions and active backend on Wayland;
- BLCVoice insertion backend selected at runtime;
- ASR engine/model identity and acceleration/backend metadata;
- microphone device identifier sufficient to reproduce the test without exposing unrelated personal data;
- permission state before the test;
- result for every scenario below;
- notes that clearly separate protocol acceptance from visible target-document verification.

## Required scenarios for every supported row

1. Happy-path shortcut-to-dictation flow completes without UI-owned business logic or privileged fallback.
2. Microphone permission denial is surfaced as an actionable outcome without crash or false success.
3. Relevant platform insertion permission denial/revocation is surfaced as unavailable/failure, not success.
4. Representative UTF-8 text is delivered, including Turkish characters (`ğüşiöç İĞÜŞÖÇ`) and emoji.
5. Multiline text is tested when the backend claims multiline support.
6. Long text is tested beyond trivial single-keystroke payload sizes.
7. Target focus is not silently redirected by BLCVoice.
8. Runtime capability resolution reports the actual selected backend and does not hide fallback behavior.
9. No unsupported root, evdev, uinput or X11 fallback is activated on Wayland.
10. Application shutdown/restart does not leave stale capture, shortcut or insertion state.

## Windows-specific validation

The native Windows path must record whether input submission succeeded and whether the intended target actually received the Unicode text. Windows `SendInput` is subject to User Interface Privilege Isolation (UIPI), so validation must include a same-integrity target and a higher-integrity target to confirm that blocked injection is reported truthfully rather than treated as successful delivery.

## macOS-specific validation

Record the exact macOS version, permission state relevant to the native insertion path, denial/revocation behavior, and semantic text delivery to at least one representative native text field. Permission prompts or accessibility-related controls must never be bypassed or described as granted when they are not.

## Wayland EIS validation

Wayland validation must use the XDG Desktop Portal RemoteDesktop flow and EIS; it must not use X11, evdev, uinput, root or a hidden XWayland fallback.

For KDE Plasma 6 and GNOME Wayland, verify:

1. BLCVoice requests keyboard control without unnecessary pointer/touchscreen permission.
2. Portal denial or cancellation returns an actionable permission/capability result without crashing.
3. After `RemoteDesktop.Start()` succeeds, BLCVoice connects through `ConnectToEIS()` and sends input exclusively through EIS for that session.
4. A successful EIS session exposes the required text/input capability or BLCVoice reports the capability as unavailable.
5. Turkish characters, emoji and text longer than 254 UTF-8 bytes are submitted without splitting a code point.
6. Focus remains on the intended target application while text is submitted.
7. Closing or revoking the portal session transitions subsequent insertion attempts to unavailable/failure.
8. Restore-token behavior, if enabled, remains revocable and does not bypass portal policy.
9. No X11, evdev, uinput or privileged fallback activates on failure.

The XDG RemoteDesktop portal documents EIS as the recommended input mechanism. After an EIS connection is established, RemoteDesktop `Notify*` input methods must not be mixed with that session.

## Claim policy

A platform may be described as runtime-validated only when its matrix row is `VALIDATED` with saved evidence tied to an exact commit. `PARTIAL` evidence may be cited narrowly (for example, “XTEST live smoke passes under Xvfb”) but must not be generalized into full dictation compatibility.

Production signing, notarization and release publication remain separate external gates and are not implied by this matrix.
