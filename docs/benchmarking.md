# Benchmarking BLCVoice

BLCVoice performance claims must be tied to a repeatable command plus the environment, model/runtime and input metadata needed to interpret the result. Hardware-sensitive timings are evidence for that environment, not universal compatibility or performance claims.

## Audio preprocessing foundation

The first deterministic benchmark exercises the engine-neutral worker-side preprocessing path with a generated five-second 48 kHz stereo signal converted to 16 kHz mono:

```bash
cargo run --release -p blcvoice-audio-processing --example preprocessing_benchmark
```

Optional controls:

```bash
cargo run --release -p blcvoice-audio-processing --example preprocessing_benchmark -- \
  --duration-seconds 10 --warm-runs 30
```

The output is line-oriented `key=value` data beginning with `format=blcvoice-benchmark-v1`. Save the complete output together with the exact repository commit when using it as evidence.

The benchmark reports construction time separately from the first processing call and warm repeated calls. It also reports real-time factor (RTF), where values below `1.0` mean the measured processing stage completed faster than the duration of the audio input on that machine.

The generated input is deterministic and contains no microphone or user data. This benchmark deliberately does not claim ASR accuracy, model-load latency, end-to-end dictation latency, memory usage, or real desktop behavior.

## Evidence rules

- Compare results only when the command, input shape, build mode and relevant runtime/model settings are equivalent.
- Record OS, architecture, logical CPU count and exact commit; future engine-specific probes must also record model identity, runtime version and acceleration backend.
- Distinguish cold construction/load from warm reuse. Do not average them together.
- CI may compile and smoke-test deterministic benchmark tooling, but CI runner timings are not a release performance baseline.
- Engine-specific measurements belong in the relevant ASR adapter or benchmark driver. The core benchmark contract must remain engine-neutral.
- Real-platform compatibility remains separate from performance benchmarking; package or benchmark success does not prove KDE Wayland, GNOME Wayland, X11, Windows or macOS runtime compatibility.

## Research basis

Criterion.rs documents warm-up, sampling, benchmark groups and throughput as distinct measurement concepts; BLCVoice follows the same separation even though the initial harness intentionally avoids adding a benchmark-framework dependency. `whisper.cpp` likewise records system/backend information and raw per-model benchmark output rather than presenting hardware-sensitive measurements without context.

Future #43 slices can add ASR cold/warm model-load and inference measurements, post-stop-to-transcript timing, and reliable memory probes without changing this evidence contract.
