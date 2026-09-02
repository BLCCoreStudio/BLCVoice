# ADR 0012: Runtime-independent dictation orchestration boundary

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice now has independent contracts for session lifecycle coordination, microphone capture, utterance finalization and speech recognition. Those pieces must be joined into one application-level dictation flow, but the desktop/Tauri layer must not become the owner of lifecycle truth or native worker resources.

The orchestration boundary also has to remain safe under concurrency. Audio finalization and speech recognition may take long enough for a user to cancel the current dictation and start another one before an old worker returns. A late worker must never mutate the newer session, clear its capture resource or deliver an old transcript.

## Decision

Introduce `blcvoice-runtime` as a runtime-independent application orchestration crate.

`blcvoice-runtime` depends only on stable BLCVoice contracts:

- `blcvoice-core` for session state and `SessionCoordinator`;
- `blcvoice-audio` for `InputCaptureFactory` and capture contracts;
- `blcvoice-dictation` for recording collection/finalization;
- `blcvoice-asr` for recognizer contracts.

It does not depend on CPAL, Tauri, transcribe.cpp or another concrete native backend.

`SessionCoordinator` remains the source of lifecycle truth. The runtime owns only ephemeral per-session work resources through a session-qualified work slot:

```text
Empty
Arming(session)
Recording(session, collector)
Finalizing(session)
Finalized(session, recording)
Transcribing(session)
```

Every mutation of this slot is qualified by `SessionId`. A late worker for an old session can therefore restore or clear only the reservation it originally owned.

## Lock ownership

The work mutex protects only short ownership/reservation changes and `RecordingCollector::pump()`. It is never held while executing:

- microphone factory startup;
- `RecordingCollector::finalize()`;
- `FinalizedRecording::transcribe()`.

The `SessionCoordinator` mutex is likewise held only inside individual coordinator calls. Native capture startup, audio finalization and ASR inference never execute under the coordinator lock.

Explicit terminal failure and cancellation first invalidate the matching ephemeral work slot, then transition the matching session to its terminal state. This prevents a newly started session from inheriting or being blocked by an old session's resource reservation. The work lock is not held during long-running finalization or ASR, so those operations do not block cancellation.

## Lifecycle mapping

The initial runtime path is:

```text
begin
  -> start capture
  -> RecordingStarted
recording
  -> pump capture handoff
stop
  -> RecordingStopped
  -> finalize audio
  -> AudioFinalized
transcribe
  -> TranscriptReady(requires_transform)
  -> Transforming | Inserting
  -> TransformFinished (when needed)
  -> Inserting
  -> InsertionDelivered
  -> Completed
```

A transcript never completes a dictation by itself. Delivery remains a separate explicit lifecycle event.

## Recognition retry policy

`FinalizedRecording` is retained while recognition is attempted. If recognition returns an error and the matching transcription reservation is still valid, the runtime restores the same finalized recording and leaves the domain session in `Transcribing`.

The caller can then choose to:

- retry recognition against the same finalized audio;
- mark speech recognition as failed;
- cancel the session.

If the session was cancelled or replaced while ASR was running, the old recording is not restored and the late worker result is rejected.

## Stale-work safety

Two independent guards apply to late work:

1. `SessionCoordinator` rejects transitions for cancelled, terminal or stale session IDs.
2. The runtime work slot allows clear/restore operations only for the session ID that owns the current reservation.

This means an old ASR worker may finish computing, but it cannot mutate a newer session or clear a newer microphone collector.

## Consequences

The runtime API is synchronous and async-runtime agnostic. Desktop/Tauri integration must schedule blocking capture/finalization/ASR work away from the UI thread rather than moving those concerns into `blcvoice-runtime`.

The separation also makes the core dictation path testable with fake capture and recognizer adapters on all CI platforms without loading a real microphone or model runtime.

## Non-goals

This decision does not add:

- Tauri commands or UI event scheduling;
- VAD or automatic endpointing;
- text insertion/platform adapters;
- model download/selection management;
- engine-native ASR cancellation;
- persistence or history.

Those concerns remain separate follow-up layers.
