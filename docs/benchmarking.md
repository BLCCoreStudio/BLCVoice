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

The generated input is deterministic and contains no microphone or user data. This benchmark deliberately does not claim ASR accuracy, model-load latency, end-to-end dictation latency, or real desktop behavior.

## transcribe.cpp ASR probe

The `transcribe.cpp` adapter owns an engine-specific benchmark driver so native model/session policy does not leak into the engine-neutral `SpeechRecognizer` contract. Run it with a local model already managed or downloaded by the user:

```bash
cargo run --release -p blcvoice-asr-transcribe --example transcribe_benchmark -- \
  --model /path/to/model --duration-seconds 2 --warm-runs 3
```

`--threads N` may be supplied to pin the adapter thread setting; zero delegates to the runtime default. The driver uses deterministic generated 16 kHz mono input and records:

- model/session load time before any inference;
- first inference latency and RTF;
- warm reused-session total/mean latency and RTF;
- engine, resolved model identity, resolved backend, thread setting and adapter version;
- OS, architecture, logical CPU count and exact input shape.

The generated signal is intended to exercise inference reproducibly, not to measure recognition accuracy. Transcript byte counts are emitted only to make completed inference observable; their text is not a quality metric. Model files are never downloaded by the benchmark and no microphone or user audio is read.

Cold model/session load and first inference are deliberately separate from warm reused-session inference. Do not combine them into a single average. A release performance claim must also retain the exact model file identity/version outside the benchmark output when `model_id` alone is insufficient to reproduce the artifact.

## Core post-stop dictation probe

The engine-neutral dictation benchmark measures the application seam beginning when recording is stopped and ending when the core pipeline returns a transcript:

```bash
cargo run --release -p blcvoice-dictation --example post_stop_benchmark
```

Optional controls:

```bash
cargo run --release -p blcvoice-dictation --example post_stop_benchmark -- \
  --duration-seconds 5 --runs 10
```

The probe constructs a deterministic in-memory 48 kHz stereo capture session, starts its timer immediately before `RecordingCollector::finalize()`, drains/finalizes the capture, preprocesses to the recognizer-required 16 kHz mono format and crosses the normal engine-neutral `SpeechRecognizer` contract. It records first and repeated finalization time plus first and repeated post-stop-to-transcript time.

The recognizer in this probe is intentionally a deterministic contract stub. Therefore this benchmark measures core finalization/preprocessing/orchestration overhead; it does **not** represent real-model ASR latency. Use the adapter-owned `transcribe_benchmark` separately for model load and real inference evidence. Do not add the two measurements together and present the sum as a measured desktop-session E2E result: microphone callback timing, scheduling and real platform behavior are outside this deterministic probe.

## Platform-qualified process memory evidence

The post-stop benchmark also records process-resident memory evidence after the timed runs. These fields are evidence about the benchmark process on that operating system; they are not a cross-platform universal "RAM usage" metric.

The line-oriented output always includes `memory_source`, `memory_current_semantics`, `memory_peak_semantics`, `memory_current_bytes` and `memory_peak_bytes`. `unavailable` is an intentional value when the platform cannot provide a trustworthy value through the selected probe.

Platform semantics:

- **Linux:** `/proc/self/status` supplies `VmRSS` for the current resident set and `VmHWM` for the resident-set high-water mark. The kernel reports these values in KiB; the benchmark normalizes only the unit to bytes while retaining the Linux semantic labels.
- **Windows:** the benchmark queries its own process through Windows PowerShell `Get-Process` and records `WorkingSet64` and `PeakWorkingSet64`. These correspond to current and peak working-set evidence, not Linux RSS/HWM semantics.
- **macOS:** `/bin/ps -o rss=` supplies a current resident-set observation. The benchmark deliberately reports in-process peak memory as unavailable. For a peak measurement, wrap the exact benchmark command with `/usr/bin/time -l`; retain the `maximum resident set size` line together with the benchmark output and exact commit.

macOS peak example:

```bash
/usr/bin/time -l cargo run --release -p blcvoice-dictation --example post_stop_benchmark \
  2>post-stop-resource.txt | tee post-stop-benchmark.txt
```

Do not compare the numeric memory fields across operating systems as if they were identical counters. Compare within the same platform and measurement source, with equivalent command, input shape, build mode and runtime settings. A hosted-runner memory value is CI evidence only, not a release-performance baseline.

The measurement strategy follows native platform semantics rather than introducing a dependency that would collapse distinct counters behind one ambiguous API. Linux exposes `VmRSS`/`VmHWM`; Windows documents current and peak working set as separate process-memory counters; macOS exposes peak resident usage through its resource-usage tooling. This is an implementation-local benchmark policy and does not change application runtime architecture, so no ADR is required.

## Evidence rules

- Compare results only when the command, input shape, build mode and relevant runtime/model settings are equivalent.
- Record OS, architecture, logical CPU count and exact commit; engine-specific probes must also record model identity, runtime/adapter version where exposed and acceleration backend.
- Distinguish cold construction/load from warm reuse. Do not average them together.
- CI may compile and smoke-test deterministic benchmark tooling, but CI runner timings and memory values are not a release performance baseline.
- Engine-specific measurements belong in the relevant ASR adapter or benchmark driver. The core benchmark contract must remain engine-neutral.
- Synthetic core post-stop measurements and real-model adapter timings are separate evidence classes; neither proves real desktop E2E latency by itself.
- Process-memory evidence must preserve its platform-specific semantic label and source. Unit normalization alone does not make counters equivalent.
- Real-platform compatibility remains separate from performance benchmarking; package or benchmark success does not prove KDE Wayland, GNOME Wayland, X11, Windows or macOS runtime compatibility.

## Research basis

Criterion.rs documents warm-up, sampling, benchmark groups and throughput as distinct measurement concepts; BLCVoice follows the same separation even though the initial harness intentionally avoids adding a benchmark-framework dependency. `whisper.cpp` likewise reports load, encode/decode and total timings, and its benchmark tooling records system/backend context rather than presenting hardware-sensitive measurements without environment information.

For resource evidence, Linux documents `/proc/pid/status` `VmRSS` and `VmHWM`; Microsoft documents `WorkingSetSize` and `PeakWorkingSetSize` as distinct process working-set counters; macOS resource-usage tooling exposes maximum resident-set information. BLCVoice therefore retains the native semantic identity instead of presenting those values as one universally comparable metric.

Issue #43 still owns later accuracy evidence before benchmark-foundation closure. Accuracy evidence must use explicit reference transcripts and a reproducible scoring method; deterministic synthetic inference input is not an accuracy corpus.
