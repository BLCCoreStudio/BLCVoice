# ADR 0009: Separate capture-to-ASR worker orchestration

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice now has independent boundaries for device-native microphone capture, bounded utterance storage, audio preprocessing, and speech recognition. The missing piece is a worker-side service that connects those boundaries without moving blocking work into the realtime audio callback or leaking CPAL/transcribe.cpp types into the application core.

The capture ring buffer is intentionally short and bounded. It is a realtime handoff, not long-term utterance storage. If application code waits until the user releases the shortcut before draining it, normal dictation can overflow even though the microphone backend itself is healthy.

The capture contract also exposes dropped-sample and callback-error counters. A transcription produced from audio that BLCVoice already knows is incomplete must not be reported as a successful dictation.

## Decision

Introduce a runtime-independent `blcvoice-dictation` worker/service crate.

Its initial push-to-talk path is:

```text
InputCaptureSession
  -> periodically drain bounded realtime handoff
  -> bounded UtteranceBuffer
  -> pause capture and drain residual audio
  -> verify capture integrity
  -> AudioPreprocessor
  -> engine-required AudioFormat
  -> SpeechRecognizer
  -> Transcription
```

The worker must be driven outside the realtime callback. `pump()` drains all currently available complete frames into bounded utterance storage. Finalization pauses capture first, drains the residual handoff, and only then preprocesses and invokes ASR.

Known capture corruption is terminal for that utterance. If `dropped_samples > 0` or `callback_errors > 0`, recognition is not attempted. An empty utterance also fails before ASR.

The recognizer's advertised required audio format determines the preprocessing target. The worker does not hard-code Whisper, 16 kHz, mono, or a specific inference backend.

`blcvoice-core` remains the owner of dictation session state and terminal pipeline semantics. `blcvoice-dictation` is an application-service/mechanics layer; it does not replace the core state machine and does not own UI state, text insertion, history, or integrations.

## Consequences

### Positive

- the realtime callback remains bounded and allocation-free,
- long utterances live in explicit bounded worker storage rather than the capture ring,
- known dropped/corrupt capture cannot become an apparently successful transcript,
- preprocessing follows engine capabilities instead of one model family,
- fake capture and fake recognizer adapters make the complete boundary deterministic to test,
- VAD can later be inserted before utterance finalization without coupling it to CPAL or transcribe.cpp.

### Trade-offs

- push-to-talk currently requires the application worker to call `pump()` regularly while recording,
- the first implementation buffers the completed utterance before one-shot ASR rather than streaming inference,
- physical microphone timing and pause/drain behavior still require hardware-backed validation,
- output amplitude policy after resampling remains enforced by the ASR input contract; out-of-range processed samples fail explicitly rather than being silently accepted.

## Deferred work

This decision does not select or implement:

- VAD/endpointing,
- streaming ASR,
- automatic worker scheduling/thread ownership,
- model lifecycle/downloads,
- text transformation or insertion,
- hardware-specific latency tuning,
- silent clipping/limiting policy for DSP overshoot.

Those require separate evidence and decisions.
