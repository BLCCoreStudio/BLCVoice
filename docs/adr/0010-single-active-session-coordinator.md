# ADR 0010: Single-active session coordination and stale-result rejection

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice has a deterministic per-session state machine and a worker-side capture-to-ASR pipeline. Those pieces intentionally do not hold one another's locks while microphone capture, DSP or recognition runs.

That separation creates a concurrency boundary: a worker can finish after the user has cancelled its dictation, or after a later dictation has already become current. Applying that late result to the current UI/insertion path would violate the core rule that stale work must never be delivered to a newly focused target.

The desktop shell will also need to mutate session state from different threads without making Tauri, an async runtime, CPAL or a recognition engine part of the domain core.

## Decision

`blcvoice-core` owns a thread-safe `SessionCoordinator` around the existing `DictationSession` state machine.

The coordinator:

- allocates monotonically increasing session IDs,
- allows only one non-terminal session at a time,
- permits a new session after the previous one is completed, failed or cancelled,
- applies lifecycle events only when their `SessionId` matches the current session,
- rejects events from replaced sessions as explicitly stale,
- allows cancellation from another thread while expensive worker work runs outside the coordinator lock,
- keeps terminal snapshots observable until a new session begins.

The coordinator lock protects only small domain-state mutations. Capture, preprocessing, model inference, downloads, transformation and text insertion must never run while that lock is held.

A cancelled session remains terminal even if an already-running worker later returns a transcript. Attempting to apply `TranscriptReady` to that cancelled session is rejected by the state machine. If a newer session has replaced it, the old session ID is rejected before any transition is attempted.

## Consequences

### Positive

- late worker results cannot mutate a newer dictation session,
- cancellation can win the domain-state race even before engine-native inference cancellation exists,
- Tauri can later share the coordinator safely without moving UI/runtime types into `blcvoice-core`,
- expensive operations stay outside a global mutex,
- overlapping dictations fail explicitly instead of silently competing for ownership.

### Trade-offs

- cancellation at this layer prevents stale delivery but does not by itself stop native ASR compute already in flight,
- the product currently supports one active dictation session per application process,
- callers must carry the returned `SessionId` across worker boundaries and submit events with it.

## Deferred work

Separate decisions are still required for:

- engine-neutral cooperative cancellation handles that can stop native ASR compute,
- the application service that binds `SessionCoordinator` to `RecordingCollector`,
- background worker/thread scheduling,
- Tauri command/event wiring,
- target application capture and text insertion.
