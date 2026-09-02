# ADR 0013: Desktop capture scheduling and IPC boundary

- Status: Accepted
- Date: 2026-09-02

## Context

`blcvoice-runtime` deliberately exposes synchronous, async-runtime-independent orchestration. While a session is recording, its `RecordingCollector` must be pumped regularly so the short realtime ring buffer remains a handoff rather than accidental utterance storage.

The Tauri desktop host now needs to compose the concrete CPAL adapter with that runtime. The unsafe architectural shortcut would be to make JavaScript poll `pump_recording()` or move raw PCM through Tauri IPC. That would couple audio integrity to webview scheduling, background throttling and UI responsiveness, and it would expose a high-volume native data path to a layer that should only display and request application state.

BLCVoice also does not yet have model download/selection management. A desktop bridge therefore must not pretend that finalized microphone audio is a completed dictation when no ASR model has been selected.

## Decision

The desktop shell owns a native `DesktopCaptureService` that composes:

- `CpalInputDeviceDiscovery` behind `InputDeviceDiscovery`;
- `CpalInputCaptureFactory` behind `InputCaptureFactory`;
- `DictationRuntime` for session and capture lifecycle truth;
- one short-lived native capture-pump worker for the active recording.

The pump worker is a named Rust thread. While recording it calls `DictationRuntime::pump_recording()` and waits 10 ms between drains. The worker and microphone stream remain entirely on the Rust side. Raw PCM is never serialized across Tauri IPC.

The desktop control slot allows only one capture worker at a time and binds that worker to the session ID that created it. A stale session cannot claim or stop a newer worker. While native capture startup is in flight, cancellation is rejected as busy until worker ownership has been installed, closing the gap between runtime session creation and desktop worker ownership.

A worker that has already terminated with a pump failure remains claimable by the matching finish/cancel path so the original failure is surfaced as `PumpFailed`; it is not silently reaped into a generic no-worker condition.

## Tauri scheduling

Potentially blocking desktop operations are exposed as async Tauri commands but executed through `tauri::async_runtime::spawn_blocking`. In particular, device discovery, native stream startup, worker shutdown and audio finalization do not execute on the UI thread.

The synchronous `desktop_status` query reads only short in-memory state and does not schedule blocking native work.

The IPC boundary returns purpose-built serializable DTOs. Concrete CPAL types, runtime mutex guards, recognizer objects and PCM buffers never cross that boundary. Error responses contain stable category codes plus human-readable messages so the frontend can provide actionable diagnostics without parsing backend strings.

## Initial microphone-test flow

Until model lifecycle management is connected, the desktop bridge exposes a bounded microphone test rather than a misleading partial dictation:

```text
discover input devices
  -> start microphone test
  -> Recording
  -> native pump worker drains capture handoff
stop test
  -> stop/join pump worker
  -> RecordingStopped
  -> finalize and integrity-check audio
  -> AudioFinalized / Transcribing
  -> capture statistics returned
  -> cancel session and discard finalized audio
```

The test is capped at 30 seconds. Its purpose is to prove device selection, native capture, realtime handoff draining and capture integrity through the same runtime path that later dictation will use.

A successful microphone test is not transcription and is not text delivery. The terminal state is deliberately `Cancelled` after the finalized audio has been measured and discarded.

## Failure handling

If the pump worker observes a runtime/capture failure, the runtime remains responsible for transitioning the session to the correct failure state. The desktop service records the worker failure for diagnostics and does not overwrite that lifecycle truth.

If worker creation itself fails, the newly started runtime session is cancelled so a native microphone stream is not left active without a consumer.

Worker ownership is session-qualified. Finish/cancel requests for another session ID are rejected rather than acting on whichever capture happens to be current.

## Consequences

- JavaScript responsiveness cannot determine whether microphone samples are drained on time.
- No raw microphone audio crosses the webview/native trust boundary.
- Tauri remains a host/composition layer rather than the owner of dictation lifecycle truth.
- The real CPAL/runtime capture path can be exercised before model management and text insertion are implemented.
- The desktop shell now requires CPAL system build dependencies in addition to Tauri prerequisites on Linux.
- Desktop CI runs controller tests and Clippy instead of compile-checking the shell only.

## Non-goals

This decision does not add:

- model download, selection or loading;
- speech recognition commands;
- VAD or automatic endpointing;
- global shortcut handling;
- text insertion;
- microphone level visualization or PCM streaming to the frontend;
- a production-ready dictation UI.

Those remain later vertical slices built on this native scheduling boundary.
