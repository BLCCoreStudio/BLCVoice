# ADR 0019: Production dictation service boundary

- Status: Accepted
- Date: 2026-09-03

## Context

The desktop host already has a microphone-test flow and a runtime that can capture, finalize, preprocess and transcribe one bounded utterance. Global shortcut registration is also implemented. Connecting shortcut decisions directly to the microphone-test API would create a false production path: microphone tests intentionally discard finalized audio and do not own recognizer preparation, ASR configuration or the pending text-insertion lifecycle.

BLCVoice also needs one exclusive microphone/capture owner. A test capture and a production dictation must not be able to start independent runtime workers against the same input device at the same time.

## Decision

The desktop host will expose a dedicated `DesktopDictationService` above the shared `DesktopCaptureService`.

The production service:

1. validates the request before recording;
2. loads and prepares the configured speech recognizer before opening the microphone;
3. starts recording through the same exclusive capture runtime used by microphone testing;
4. binds the prepared recognizer and recognition options to that session;
5. on stop, finalizes audio before beginning ASR;
6. preprocesses/resamples only through the existing runtime/dictation pipeline;
7. transcribes through the engine-agnostic `SpeechRecognizer` contract;
8. leaves a successful transcript in the domain `Inserting` state until a real text-insertion adapter confirms delivery;
9. rejects stale session IDs and overlapping preparation/recording/finalization work;
10. fails recognition explicitly and clears retained finalized audio when transcription cannot complete.

The initial desktop implementation uses the existing `transcribe.cpp` adapter through a recognizer factory. The factory boundary is retained so tests and future engines do not require the desktop service to depend on one concrete recognizer internally.

The initial maximum production utterance duration is five minutes. This remains a bounded dictation default, not a meeting-recording feature.

## Consequences

- A missing or unloadable model is detected before recording begins.
- Microphone testing and real dictation share one capture owner and cannot overlap.
- The desktop now has a real capture-to-ASR path rather than a renamed microphone test.
- Successful transcription is not reported as completed dictation until text insertion confirms delivery.
- Shortcut decisions remain decoupled from production capture until the insertion layer exists; this prevents a global shortcut from starting a flow that cannot yet complete its advertised user action.
- Model discovery/download/recommendation remains a separate future subsystem. This ADR only accepts a model path supplied by the trusted desktop frontend/runtime configuration.
