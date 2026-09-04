# Benchmarking BLCVoice

BLCVoice performance and accuracy claims must be tied to a repeatable command plus the environment, model/runtime and input metadata needed to interpret the result. Hardware-sensitive timings and resource counters are evidence for that environment, not universal compatibility or performance claims.

## Audio preprocessing foundation

The deterministic preprocessing benchmark exercises the engine-neutral worker-side path with generated 48 kHz stereo audio converted to 16 kHz mono:

```bash
cargo run --release -p blcvoice-audio-processing --example preprocessing_benchmark
```

Optional controls:

```bash
cargo run --release -p blcvoice-audio-processing --example preprocessing_benchmark -- \
  --duration-seconds 10 --warm-runs 30
```

Output is line-oriented `key=value` data beginning with `format=blcvoice-benchmark-v1`. Save complete output with the exact repository commit when using it as evidence. Construction, first processing and warm repeated calls remain separate. RTF below `1.0` means that measured stage completed faster than the duration of the input on that machine.

The generated input contains no microphone or user data and deliberately does not claim ASR accuracy, model-load latency, desktop E2E latency or memory use.

## transcribe.cpp ASR probe

The `transcribe.cpp` adapter owns its engine-specific benchmark so native model/session policy does not leak into the engine-neutral `SpeechRecognizer` contract:

```bash
cargo run --release -p blcvoice-asr-transcribe --example transcribe_benchmark -- \
  --model /path/to/model --duration-seconds 2 --warm-runs 3
```

`--threads N` may pin the adapter thread setting; zero delegates to the runtime default. The driver uses deterministic generated 16 kHz mono input and records model/session load, first inference latency/RTF, warm reused-session latency/RTF, engine/model/backend/thread metadata, OS/architecture/logical CPUs and exact input shape.

The generated signal exercises inference reproducibly; transcript byte counts are observability only, not recognition-quality evidence. Model files are never downloaded by this benchmark and no microphone or user audio is read. Cold model/session load and first inference must not be averaged with warm reused-session inference.

## Core post-stop dictation probe

The engine-neutral dictation benchmark measures the seam beginning when recording is stopped and ending when the core pipeline returns a transcript:

```bash
cargo run --release -p blcvoice-dictation --example post_stop_benchmark
```

Optional controls:

```bash
cargo run --release -p blcvoice-dictation --example post_stop_benchmark -- \
  --duration-seconds 5 --runs 10
```

The probe uses deterministic in-memory 48 kHz stereo capture, starts timing immediately before `RecordingCollector::finalize()`, preprocesses to the recognizer-required 16 kHz mono format and crosses the normal engine-neutral `SpeechRecognizer` contract. It records first/repeated finalization and post-stop-to-transcript time.

Its recognizer is intentionally a deterministic contract stub. It measures core finalization/preprocessing/orchestration overhead, not real-model ASR latency. Do not add this number to adapter timing and present the sum as measured desktop-session E2E latency; microphone callbacks, scheduling and real platform behavior are outside this deterministic probe.

## Platform-qualified peak-memory evidence

Memory counters are not normalized into a fictitious universal "RAM usage" metric. Thin wrappers use native process accounting and retain the platform semantic in the emitted key name. Build the benchmark executable first so the wrapper measures the benchmark process rather than Cargo/compiler subprocesses:

```bash
cargo build --release -p blcvoice-asr-transcribe --example transcribe_benchmark
```

Linux or macOS:

```bash
bash scripts/measure-resource.sh -- \
  target/release/examples/transcribe_benchmark \
  --model /path/to/model --duration-seconds 2 --warm-runs 3 \
  2>&1 | tee benchmark-resource.txt
```

Windows PowerShell 7+:

```powershell
cargo build --release -p blcvoice-asr-transcribe --example transcribe_benchmark
& ./scripts/measure-resource.ps1 \
  -FilePath ./target/release/examples/transcribe_benchmark.exe \
  -ArgumentList @('--model', 'C:\path\to\model', '--duration-seconds', '2', '--warm-runs', '3') |
  Tee-Object benchmark-resource.txt
```

The wrapper appends `resource_format=blcvoice-resource-evidence-v1` plus one platform-qualified peak metric:

- Linux: `linux_peak_resident_set_kib`, preserving GNU `/usr/bin/time -v` / Linux `getrusage` high-water semantics and KiB units;
- macOS: `macos_peak_resident_set_bytes`, preserving `/usr/bin/time -l` native maximum-resident-set semantics and byte units;
- Windows: `windows_peak_working_set_bytes`, preserving `PeakWorkingSetSize` / `.NET PeakWorkingSet64` semantics and byte units.

Do not numerically compare these fields as though they were the same operating-system accounting concept. Within-platform comparisons still require the same OS/runtime, model, backend, command, input shape, build mode and comparable background conditions. A failed measured command remains a failed evidence run because wrappers preserve its exit code.

Linux separately documents `VmRSS` as current resident memory and `VmHWM` as the peak resident-set high-water mark. Windows documents `WorkingSetSize` and `PeakWorkingSetSize` as current and peak working-set bytes. macOS exposes native resident-memory accounting including `resident_size_peak`. BLCVoice therefore records platform semantics instead of collapsing them into one cross-platform label.

## Accuracy evidence

`scripts/score-accuracy.py` provides a deterministic WER/CER scorer for real reference/hypothesis evidence. Input is JSON Lines:

```json
{"id":"sample-001","reference":"Merhaba dünya","hypothesis":"Merhaba dünya"}
```

Run:

```bash
python3 scripts/score-accuracy.py accuracy-input.jsonl | tee accuracy-score.txt
```

The scorer applies Unicode NFC only, then whitespace tokenization for WER and Unicode code points for CER. Case and punctuation remain significant. Output begins with `format=blcvoice-accuracy-v1` and records the normalization policy, sample count, reference units, edit counts, WER and CER.

A score is valid model-quality evidence only when retained with the exact model artifact/identity, engine/runtime/backend settings, dataset name/version/source/license, language/subset, reference preparation rules, BLCVoice commit and the hypothesis-generation command. The scorer never downloads a dataset and deterministic CI runs only `--self-test`; CI therefore validates scoring logic but creates no product accuracy claim. Private/user audio must not be introduced merely to satisfy a benchmark target.

## Evidence rules

- Compare results only when command, input shape, build mode and relevant runtime/model settings are equivalent.
- Record OS, architecture, logical CPU count and exact commit; engine-specific probes also record model identity, runtime/adapter version where exposed and acceleration backend.
- Keep cold construction/load separate from warm reuse.
- CI may compile/smoke-test deterministic tooling, but hosted-runner timing and memory readings are not release baselines.
- Engine-specific measurements stay in adapters/benchmark drivers; core benchmark contracts remain engine-neutral.
- Synthetic post-stop measurements and real-model adapter timings are separate evidence classes; neither proves desktop E2E latency.
- Platform-native resource counters retain native semantic/unit; do not normalize unlike counters solely for a cross-platform chart.
- Accuracy scores without reproducible model/dataset provenance are not release-quality evidence.
- Real-platform compatibility remains separate from benchmarking; package or benchmark success does not prove KDE Wayland, GNOME Wayland, X11, Windows or macOS runtime compatibility.

## Research basis

Criterion.rs separates warm-up, sampling, benchmark groups and throughput; BLCVoice follows the same conceptual separation without requiring a benchmark-framework dependency for the initial harness. `whisper.cpp` similarly exposes load/inference timing and backend/system context rather than presenting hardware-sensitive measurements without environment information.

Resource accounting follows the Linux kernel's `/proc/PID/status` definitions, Microsoft's `PROCESS_MEMORY_COUNTERS(_EX)` working-set definitions and Apple's native resident-memory accounting. Wrappers intentionally use native tools/APIs rather than adding unsafe FFI or an application runtime dependency solely for benchmark telemetry.

WER/CER tooling is deliberately independent of the ASR adapter. The evidence producer (model/runtime inference) and evidence scorer are separate so recognition quality can be compared across engines without changing the application core.
