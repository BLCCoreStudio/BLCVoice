# ADR 0007: Bounded utterance processing before ASR

- Status: Accepted
- Date: 2026-09-02

## Context

ADR 0006 established a fixed-block, engine-driven audio preprocessing primitive. That primitive is useful for future incremental processing, but a dictation product also needs an exact utterance-end path: the last microphone read is rarely an exact resampler block, and sample-rate conversion has filter delay that must be flushed and trimmed correctly.

Dropping the final partial block can remove the end of a spoken word. Manually streaming zero-padding without accounting for startup delay can introduce leading silence, trailing padding or incorrect output length.

BLCVoice v0.1 is primarily a push-to-talk dictation path. Correctness at the utterance boundary is more important than prematurely exposing partial ASR output.

## Decision

Add two worker-side primitives to `blcvoice-audio-processing`:

1. `UtteranceBuffer`
   - accumulates device-native interleaved `f32` samples;
   - accepts only complete interleaved frames;
   - has an explicit maximum frame/duration limit so a stuck recording cannot grow memory without bound;
   - retains allocated capacity across `clear()` for reuse;
   - is never called from the CPAL realtime callback.

2. `AudioPreprocessor::process_utterance`
   - accepts any complete-frame utterance length, not only the fixed streaming block size;
   - applies the same deterministic channel-normalization policy as block processing;
   - uses Rubato `process_all_into_buffer()` for resampled utterances;
   - resets the resampler at utterance boundaries;
   - relies on Rubato's whole-clip path to process a short final chunk, flush the filter tail and trim startup delay;
   - returns only the exact valid output frames through processor-owned reusable storage.

Internal utterance scratch/output vectors grow on demand and keep their capacity for later utterances. Allocation is therefore allowed on the normal worker thread but remains forbidden from the realtime capture callback.

## Short-utterance regression found during validation

Validation against Rubato 5.0.0 found an important edge case for the chosen `Fft` configuration: a 100-frame 48 kHz clip, shorter than the configured 960-frame input chunk, returned the expected 34-frame output length through `process_all_into_buffer()` but the samples were all silence. A 1000-frame clip with a 40-frame final partial chunk produced useful output.

BLCVoice therefore does not delegate utterance delay handling to the whole-clip helper. It uses the maintained `process_into_buffer()` primitive directly, treats `output_delay()` as an explicit prefix to withhold, handles the final partial input with `Indexing::partial_len`, pumps zero-length input only to drain filter delay, and finally exposes exactly `ceil(resample_ratio * input_frames)` frames after the delayed prefix. Dedicated tests cover both shorter-than-one-chunk and final-partial utterances.

## Why not implement incremental ASR here

A correct live stream needs an additional contract for:

- when delayed resampler output becomes valid enough to emit;
- how startup delay is withheld exactly once;
- how final filter state is drained without emitting padding;
- how VAD/utterance state interacts with partial ASR hypotheses;
- backpressure between capture, preprocessing and inference.

Those concerns are materially different from exact push-to-talk utterance processing. Implementing them now would increase complexity before the first ASR backend exists and would make correctness harder to prove.

The fixed-block `process_block` API remains available as the low-level building block for that future incremental path.

## Consequences

Positive:

- short final speech is preserved rather than silently discarded;
- startup/filter delay handling follows the resampler's maintained whole-clip implementation instead of a BLCVoice-specific approximation;
- memory growth is bounded by an explicit utterance limit;
- repeated dictations reuse allocated buffers and reset DSP state cleanly;
- the next ASR adapter can consume one exact target-format utterance.

Trade-offs:

- v0.1 preprocessing does not emit live partial audio to ASR while the user is still speaking;
- longer utterances remain resident in memory until processing finishes;
- product/orchestration code must choose a reasonable maximum utterance duration for its use case.

## Follow-up gate

Before claiming live/streaming transcription, add a separately tested incremental pipeline that proves startup-delay trimming, final draining and backpressure behavior under continuous capture. Do not implement live output by repeatedly calling whole-clip processing on a growing buffer.
