# ADR 0005: Real-time audio handoff

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice needs to move microphone samples from an operating-system audio callback into ordinary application workers without making the callback wait on ASR, VAD, UI, disk, allocation-heavy processing, or a contended lock.

CPAL 0.18 invokes the data callback from a dedicated/high-priority audio path on modern desktop backends and now returns streams paused on every backend. A capture implementation therefore needs an explicit `play()` transition and a handoff that returns immediately under downstream pressure.

## Decision

Use a fixed-capacity single-producer/single-consumer ring buffer between the CPAL input callback and downstream capture workers.

For the initial implementation:

- use `rtrb` as the SPSC queue implementation;
- allocate the complete queue before the stream starts;
- normalize PCM sample representation to interleaved `f32` inside the callback;
- preserve the device-native channel count and sample rate;
- treat an interleaved audio frame as an indivisible unit across queue capacity, overflow and consumer reads;
- defer downmixing, resampling, VAD and ASR to downstream workers;
- never block or wait for free queue capacity in the data callback;
- when the queue is full, retain the samples that fit, drop only the remainder, and increment an explicit dropped-sample counter;
- record callback error categories through atomics rather than a mutex or blocking channel;
- reject DSD input formats from the dictation capture path instead of pretending they are ordinary PCM;
- keep CPAL and `rtrb` behind `blcvoice-audio` runtime-independent capture traits.

The default handoff capacity is one second of device-native interleaved samples. The domain contract caps the configurable duration at five seconds so a configuration mistake cannot silently allocate an unbounded queue.

## Why `rtrb`

`rtrb` is purpose-built for real-time SPSC communication. It allocates a fixed-capacity buffer at construction and its producer/consumer operations are lock-free and wait-free after that allocation. This is a better fit for an audio callback than an unbounded channel or a mutex-protected `Vec`.

## Overflow policy

Overflow is observable data loss, not backpressure.

Blocking the audio callback until the consumer catches up risks xruns, capture stalls and platform-specific instability. BLCVoice therefore records `received_samples` and `dropped_samples` so diagnostics and later recovery policy can distinguish a clean capture from an overloaded pipeline. For multi-channel PCM, overflow may only commit complete interleaved frames; a partial frame is dropped rather than shifting channel alignment for all later audio.

A non-zero dropped-sample count must never be reported as a fully healthy recording session.

## Consequences

### Positive

- the callback has a bounded amount of work;
- downstream ASR/VAD latency cannot directly block the audio thread;
- memory use is bounded before capture starts;
- overflow is measurable instead of hidden;
- the core remains independent of CPAL and the ring-buffer crate;
- sample-format conversion is centralized at the adapter boundary.

### Trade-offs

- native multi-channel audio is still interleaved and must be downmixed later;
- native sample rates still require a resampling stage before engines that expect a fixed rate;
- converting every PCM sample to `f32` consumes some callback CPU;
- a fixed queue can overflow if the consumer is stalled for longer than its capacity;
- CI can validate API/build behavior but cannot prove real-device timing, permission prompts, disconnect recovery or xrun behavior.

## Follow-up validation

Before calling capture production-ready, test on physical devices for at least:

- KDE Wayland / PipeWire;
- GNOME Wayland / PipeWire;
- Windows WASAPI;
- macOS CoreAudio when physical Mac testing is available;
- microphone unplug/replug while recording;
- default-device changes during capture;
- device-busy and permission-denied paths;
- sustained capture while intentionally slowing the consumer to verify overflow metrics;
- long-running capture for memory growth and callback stability.
