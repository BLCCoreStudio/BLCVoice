# Platform validation research basis

This note records the external platform contracts that constrain BLCVoice runtime validation. It is supporting rationale for `docs/platform-validation.md`, not a separate compatibility truth source.

## Wayland / XDG Desktop Portal

The XDG Desktop Portal RemoteDesktop interface requires a session lifecycle of `CreateSession` -> `SelectDevices` -> `Start`. For EIS input, `ConnectToEIS()` is called after a successful `Start()` and yields a file descriptor used by a libei sender context. Once EIS is connected for the session, input must be sent through EIS rather than mixed with RemoteDesktop `Notify*` methods.

BLCVoice therefore treats portal acceptance, EIS connection and semantic target delivery as separate evidence points. A portal/backend accepting the session is not itself proof that text reached the intended document.

## Windows

Microsoft documents `SendInput` as serial insertion of synthetic input events and notes that it is subject to User Interface Privilege Isolation (UIPI). Input injection is permitted only into applications at an equal or lower integrity level, and the API does not reliably identify UIPI blocking through its normal error channel. BLCVoice validation must therefore compare the adapter outcome with observable target delivery and include a higher-integrity target case.

## Validation consequence

The runtime matrix deliberately avoids a binary "API returned success" definition. A support claim requires both truthful capability/outcome reporting and observable application-level behavior on the exact recorded environment.
