# ADR 0008: ASR contracts and the first transcribe.cpp adapter

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice now has device-native capture, bounded utterance buffering and an engine-driven preprocessing boundary. The next layer must turn processed audio into text without making the application core depend on one model family or native runtime.

A first runtime is still necessary to prove the end-to-end dictation path. Choosing that runtime must not turn its model IDs, backend flags, result structs or error taxonomy into BLCVoice-wide concepts.

## Decision

Create two separate crates:

- `blcvoice-asr`: runtime-independent speech-recognition contracts;
- `blcvoice-asr-transcribe`: the first native adapter, backed by `transcribe.cpp`.

The application-facing contract owns normalized PCM validation, requested task/language/timestamp controls, engine capabilities, engine-neutral transcript rows and typed recognition failures. Native `transcribe.cpp` types stay inside the adapter.

The first adapter uses `transcribe-cpp` 0.2.3 with `default-features = false`. The default BLCVoice build therefore does not silently compile a platform GPU backend. Metal, Vulkan, CUDA, ROCm and OpenMP are explicit Cargo features on the adapter and can be enabled by packaging targets that have been validated for them.

`transcribe.cpp` is the first adapter because it provides an official safe Rust binding over a GGML/GGUF runtime, supports multiple ASR model families, exposes model capabilities and typed native errors, and can use CPU as well as several optional acceleration backends. This is an implementation choice for the first runtime, not a permanent architectural dependency.

## Audio contract

An ASR adapter advertises its required channel count and sample rate. BLCVoice preprocessing converts the completed device-native utterance to that shape before recognition.

The ASR contract does not hard-code a global 16 kHz model format. The transcribe.cpp adapter derives the required rate from loaded model metadata and currently requests mono input because the native API accepts mono PCM. A future adapter may advertise a different signal shape without changing capture or application-core semantics.

Normalized ASR PCM is finite interleaved `f32` in `[-1, 1]`. Invalid or frame-misaligned input fails before entering native inference.

## Capability and failure boundaries

The adapter maps native capabilities into engine-neutral metadata including languages, translation support, streaming support, cancellation support, timestamp granularity, punctuation/ITN controls and maximum audio duration when advertised.

Native failures remain typed. Missing models, model-load failures, unavailable compute backends, out-of-memory conditions, unsupported requests, overlong audio, cancellation, busy models and truncated output are not collapsed into one generic error. Partial transcripts from cancellation or truncation are preserved.

## Concurrency

`transcribe.cpp` 0.x serializes compute per loaded model. BLCVoice does not pretend one model instance provides parallel inference. If later benchmarks justify multiple workers, the runtime layer can own multiple model instances without changing the application-facing ASR contract.

## Deferred work

This decision deliberately does not add:

- model discovery, downloading, verification or recommendation policy;
- VAD or speech segmentation policy;
- streaming/incremental dictation;
- generic cancellation wiring from the application session into every engine;
- GPU packaging defaults;
- real-model accuracy, latency or memory benchmarks;
- UI model selection.

Those require separate evidence and decisions. The first acceptance gate for this adapter is clean multi-platform compilation plus unit coverage of the engine boundary. A later hardware/model smoke suite must validate actual inference before BLCVoice claims a model/runtime combination is supported.

## Consequences

BLCVoice gains one concrete local ASR path while keeping the core replaceable. The cost is an additional adapter layer and native C++ build time in CI, but model/runtime churn stays isolated from dictation orchestration and platform code.
