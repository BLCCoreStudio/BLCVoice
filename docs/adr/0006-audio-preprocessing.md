# ADR 0006: Engine-driven audio preprocessing

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice capture intentionally exposes device-native channel count and sample rate after normalizing only the numeric PCM representation to interleaved `f32`.

Speech-recognition runtimes commonly require a different signal shape. Many current local ASR models consume mono 16 kHz audio, but that is an engine requirement rather than a universal property of dictation. Hard-coding 16 kHz mono into capture would couple device I/O to whichever engine is integrated first and make future model backends harder to support correctly.

The preprocessing path must also remain outside the CPAL callback. Sample-rate conversion can be CPU-intensive and must not increase callback latency or xrun risk.

## Decision

Create a separate `blcvoice-audio-processing` crate between native capture and ASR adapters.

The initial processing primitive:

- accepts device-native interleaved `f32` PCM;
- takes explicit source and target channel/rate formats;
- preserves channels when source and target channel counts match;
- supports deterministic N-channel to mono conversion by uniformly averaging each complete frame;
- rejects other channel-count transformations rather than inventing an implicit upmix or remapping policy;
- uses Rubato's fixed-rate FFT resampler when source and target sample rates differ;
- keeps a reusable resampler and preallocated scratch buffers across blocks;
- exposes the required input block size and maximum output size so orchestration can preallocate buffers;
- does not execute in the audio callback.

The default processing chunk is 1024 source frames. It is an implementation default, not an ASR sample-rate policy.

## Why Rubato FFT

BLCVoice converts between a device rate and an engine-requested rate that are fixed for the lifetime of a processing session. Rubato 5's synchronous `Fft` resampler is designed for fixed ratios and provides high-quality anti-aliased conversion. Its `process_into_buffer` API writes into caller-owned output storage and is suitable for reusable streaming workers.

Rubato is licensed `MIT OR Apache-2.0`, matching the BLCVoice workspace license policy.

## Block and utterance boundaries

`AudioPreprocessor::process_block` deliberately handles complete reusable blocks only. A later capture-processing worker owns accumulation of device frames and explicit final-partial flushing at utterance boundaries.

This separation prevents the DSP primitive from guessing when speech has ended and avoids silently padding or discarding an incomplete final utterance.

Before ASR integration is considered complete, the orchestration layer must prove that final partial audio is flushed correctly.

## Consequences

Positive:

- capture remains backend-focused and engine-agnostic;
- ASR adapters can request their actual signal format instead of relying on a global 16 kHz assumption;
- resampling and downmix policy are testable without physical audio hardware;
- fixed reusable buffers make processing latency and allocation behavior easier to observe and benchmark.

Trade-offs:

- uniform averaging is intentionally conservative and is not a full microphone-array mixer;
- arbitrary channel remapping is unsupported initially;
- FFT resampling introduces algorithmic delay that diagnostics and latency benchmarks must account for;
- an orchestration buffer is still required between realtime capture and this fixed-block DSP primitive.
