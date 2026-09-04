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

## Platform-qualified peak-memory evidence

Memory counters are not normalized into a fictitious universal "RAM usage" metric. The repository provides thin wrappers around the operating system's native process accounting and keeps the platform semantic in the emitted key name.

Build the benchmark executable first so the resource wrapper measures the benchmark process rather than Cargo and its compiler subprocesses:

```bash
cargo build --release -p blcvoice-asr-transcribe --example transcribe_benchmark
```

On Linux or macOS:

```bash
scripts/measure-resource.sh -- \
  target/release/examples/transcribe_benchmark \
  --model /path/to/model --duration-seconds 2 --warm-runs 3 \
  2>&1 | tee benchmark-resource.txt
```

On Windows PowerShell 7+:

```powershell
cargo build --release -p blcvoice-asr-transcribe --example transcribe_benchmark
./scripts/measure-resource.ps1 \
  ./target/release/examples/transcribe_benchmark.exe \
  --model C:\path\to\model --duration-seconds 2 --warm-runs 3 |
  Tee-Object benchmark-resource.txt
```

The wrapper appends `resource_format=blcvoice-resource-evidence-v1` plus one platform-qualified peak metric:

- Linux: `linux_peak_resident_set_kib`, preserving GNU `/usr/bin/time -v` / Linux `getrusage` high-water semantics and KiB units;
- macOS: `macos_peak_resident_set_bytes`, preserving `/usr/bin/time -l` native maximum-resident-set semantics and byte units;
- Windows: `windows_peak_working_set_bytes`, preserving `PeakWorkingSetSize` / `.NET PeakWorkingSet64` semantics and byte units.

Do not numerically compare these fields as though they were the same operating-system accounting concept. Within-platform comparisons still require the same OS/runtime, model, backend, command, input shape, build mode and comparable background conditions. The wrappers preserve the measured command's exit code; a failed benchmark cannot be turned into valid resource evidence.

For Linux, the kernel separately documents `VmRSS` as current resident memory and `VmHWM` as peak resident-set high-water mark in `/proc/PID/status`. Windows documents `WorkingSetSize` and `PeakWorkingSetSize` as current and peak working-set bytes. macOS exposes native resident-memory counters including `resident_size_peak`. Those sources are why BLCVoice records the platform semantic rather than collapsing all three into one cross-platform label.

## Evidence rules

- Compare results only when the command, input shape, build mode and relevant runtime/model settings are equivalent.
- Record OS, architecture, logical CPU count and exact commit; engine-specific probes must also record model identity, runtime/adapter version where exposed and acceleration backend.
- Distinguish cold construction/load from warm reuse. Do not average them together.
- CI may compile and smoke-test deterministic benchmark tooling, but CI runner timings and memory readings are not a release performance baseline.
- Engine-specific measurements belong in the relevant ASR adapter or benchmark driver. The core benchmark contract must remain engine-neutral.
- Synthetic core post-stop measurements and real-model adapter timings are separate evidence classes; neither proves real desktop E2E latency by itself.
- Platform-native resource counters retain their native semantic and unit; do not normalize unlike counters solely to make a cross-platform chart.
- Real-platform compatibility remains separate from performance benchmarking; package or benchmark success does not prove KDE Wayland, GNOME Wayland, X11, Windows or macOS runtime compatibility.

## Research basis

Criterion.rs documents warm-up, sampling, benchmark groups and throughput as distinct measurement concepts; BLCVoice follows the same separation even though the initial harness intentionally avoids adding a benchmark-framework dependency. `whisper.cpp` likewise reports load, encode/decode and total timings, and its benchmark tooling records system/backend context rather than presenting hardware-sensitive measurements without environment information.

For resource accounting, the implementation follows the Linux kernel's `/proc/PID/status` definitions, Microsoft's `PROCESS_MEMORY_COUNTERS(_EX)` working-set definitions, and Apple's native resident-memory accounting. The wrappers intentionally use platform-native tools/APIs rather than adding a runtime dependency or unsafe FFI to the application merely for benchmark telemetry.

Issue #43 still owns later accuracy evidence. Resource tooling alone does not establish model quality, a production memory budget, or cross-platform runtime compatibility.
