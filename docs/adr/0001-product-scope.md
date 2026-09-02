# ADR 0001: Start with a focused universal dictation product

- Status: Accepted
- Date: 2026-09-02

## Context

The desktop voice market already includes mature products with local transcription, context awareness, meetings, agents and many model choices. Competing on feature count from the first release would increase implementation risk without proving that BLCVoice solves the core interaction better.

The product must be understandable to a new user without requiring knowledge of ASR runtimes, quantization, GPU backends or agent protocols.

## Decision

BLCVoice will first optimize the universal dictation loop:

```text
press -> speak -> transcribe -> insert -> done
```

The initial usable milestone prioritizes:

- global push-to-talk,
- dependable microphone capture,
- local ASR,
- VAD,
- model/backend recommendation,
- text insertion,
- lightweight feedback UI,
- basic local history,
- diagnostics.

Deep agent/editor integrations, meeting features and broad automation are deferred until the universal path is reliable.

## Consequences

### Positive

- Smaller surface area for the first usable release.
- Reliability work is not displaced by feature competition.
- New users can understand the product quickly.
- Later integrations can build on a stable core instead of becoming part of it.

### Negative

- Early releases may appear less feature-rich than established competitors.
- Some differentiating integration ideas will not ship immediately.

## Revisit when

This decision should be revisited after the core dictation path is demonstrably reliable on supported platforms and there is evidence that users need deeper workflows.
