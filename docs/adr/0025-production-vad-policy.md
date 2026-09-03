# ADR 0025: Production VAD policy

## Status

Accepted.

## Decision

Production dictation runs engine-agnostic Silero VAD after capture finalization and before ASR.

- VAD analyzes a mono view at the native capture sample rate.
- If no speech is detected, the session terminates cleanly as a no-op: ASR and text insertion are skipped.
- If speech is detected, only leading and trailing silence outside the outer speech envelope is removed before ASR.
- Silence between speech regions is preserved; BLCVoice does not concatenate VAD segments.
- Speech-detection backend failures are reported as `SpeechDetection`, not mislabeled as recognition failures.
- The existing maximum dictation duration remains the hard safety bound. Streaming automatic endpointing is a separate policy layer.

## Rationale

This prevents silent clips from reaching an ASR engine and reduces unnecessary context without erasing natural pauses. Keeping VAD outside transcribe.cpp preserves the engine-agnostic ASR contract and supports model families that do not expose native VAD.

## Non-goals

This decision does not add meeting segmentation, diarization, continuous listening, or automatic stop-on-silence.
