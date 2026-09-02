# ADR 0011: Separate audio finalization from speech recognition

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice models dictation as explicit lifecycle states. After recording stops, the core state machine moves through `FinalizingAudio` and then `Transcribing` before a transcript can be accepted.

The first capture-to-ASR worker exposed `RecordingCollector::finish()`, which correctly paused capture, drained residual audio, validated capture integrity, preprocessed the utterance and ran recognition. However, that single call combined two lifecycle boundaries with very different timing and recovery semantics:

1. microphone/audio finalization;
2. potentially long-running speech recognition.

A runtime using only that API could not truthfully emit `AudioFinalized` until recognition had already completed. The session would therefore remain in `FinalizingAudio` while the recognizer was actually decoding. This would make UI state, cancellation policy, diagnostics and latency measurement inaccurate.

## Decision

Introduce an explicit `FinalizedRecording` boundary in `blcvoice-dictation`.

`RecordingCollector::finalize()` now performs only capture-side finalization:

1. pause the input stream;
2. drain the residual realtime handoff;
3. reject known sample loss or callback errors;
4. reject an empty utterance;
5. return an owned `FinalizedRecording`.

`FinalizedRecording::transcribe()` performs recognizer-dependent work:

1. inspect recognizer-required audio format;
2. preprocess the finalized source audio;
3. validate the ASR input contract;
4. invoke the recognizer;
5. return `CaptureTranscription` with source/ASR frame counts and capture statistics.

`RecordingCollector::finish()` remains as a compatibility convenience and delegates to `finalize()?.transcribe(...)`.

`FinalizedRecording::transcribe()` borrows the finalized recording rather than consuming it. This permits policy-controlled recognition retries without reopening the microphone or copying the source utterance merely to preserve a retry path.

## Lifecycle mapping

Runtime orchestration should use the boundary as follows:

```text
Recording
  -> RecordingStopped
FinalizingAudio
  -> RecordingCollector::finalize()
  -> AudioFinalized
Transcribing
  -> FinalizedRecording::transcribe()
  -> TranscriptReady
```

The session coordinator remains responsible for rejecting late or stale worker results. This ADR does not make engine-native inference cancellable by itself.

## Consequences

### Positive

- UI and diagnostics can report the real active pipeline stage.
- Audio-finalization latency and ASR latency can be measured independently.
- Recognition can be retried against one integrity-checked recording without recapture.
- Capture errors are classified before recognizer work begins.
- Runtime orchestration gets a clean seam without depending on CPAL or transcribe.cpp types.

### Costs

- One additional owned application value (`FinalizedRecording`) exists between capture and ASR.
- Finalized source audio remains in memory for as long as recognition/retry policy retains the value.
- Runtime code must explicitly advance the core state after successful finalization.

## Non-goals

This decision does not add:

- VAD or streaming ASR;
- engine-native cancellation handles;
- model loading or model management;
- text transformation or insertion;
- persistence of raw microphone audio;
- Tauri commands or UI wiring.
