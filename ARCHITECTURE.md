# BLCVoice Architecture

This document defines the initial architecture boundaries for BLCVoice. It is intentionally more conservative than a feature roadmap: its purpose is to keep the core replaceable, testable and platform-aware while the product is still small.

## 1. Core user path

The first-class pipeline is:

```text
shortcut
  -> capture audio
  -> detect speech
  -> transcribe
  -> normalize/transform (optional)
  -> resolve target/capabilities
  -> insert text
  -> report outcome
```

Every stage must have an explicit result. A later stage must not turn a failed earlier stage into a generic success state.

## 2. Architectural rules

### 2.1 The core does not depend on one ASR engine

Speech runtimes are adapters behind a stable engine interface. Engine-specific model IDs, backend flags and implementation details must not leak into unrelated product layers.

The interface should eventually expose capabilities such as:

- batch transcription,
- streaming transcription,
- supported languages,
- word timestamps,
- language detection,
- hotword/context support,
- hardware backends,
- model resource requirements.

### 2.2 Platform behavior is capability-driven

`Windows`, `Linux` and `macOS` are not sufficient capability descriptions. Linux/X11, KDE Wayland, GNOME Wayland and other compositor environments may provide different shortcut, clipboard, accessibility and insertion mechanisms.

Platform adapters should expose discovered capabilities rather than pretending a feature is universally present.

Example conceptual result:

```text
global_shortcut: available(portal)
clipboard: available(native)
text_insertion: available(accessibility)
active_application: available
selected_text: unavailable
```

### 2.3 Attempted is not delivered

Text insertion is a pipeline with its own state. A successful transcription followed by a failed paste is not a successful dictation.

Where the platform permits it, BLCVoice should distinguish:

- target resolved,
- insertion method selected,
- insertion attempted,
- delivery verified or unverifiable,
- fallback attempted,
- terminal failure.

### 2.4 The UI is not the business-logic boundary

The desktop UI should orchestrate and display state, not own speech inference, platform policy, storage semantics or integration permissions.

### 2.5 External integrations are adapters

Claude Code, Codex, VS Code, Cursor, Zed and future integrations must not become dependencies of the dictation core.

The intended layering is:

```text
                         +------------------+
                         |   Desktop UI     |
                         +--------+---------+
                                  |
                         +--------v---------+
                         | Application Core |
                         +---+----+----+-----+
                             |    |    |
             +---------------+    |    +----------------+
             |                    |                     |
      +------v------+      +------v------+      +-------v-------+
      | ASR Adapter |      | Platform    |      | Integration   |
      | Interface   |      | Capabilities|      | Gateway       |
      +-------------+      +-------------+      +-------+-------+
                                                        |
                                            +-----------+-----------+
                                            | MCP / IPC / CLI / SDK |
                                            +-----------------------+
```

## 3. Proposed module boundaries

Names are provisional until implementation starts, but responsibilities should remain separated.

### `core`

Owns dictation session state, orchestration and domain-level results. It should not contain operating-system API calls or model-runtime implementation code.

### `audio`

Microphone enumeration, capture, buffering, sample conversion and device-change handling.

### `vad`

Voice activity detection and segmentation policy.

### `asr`

Stable engine contracts, model capability metadata and transcription requests/results.

### `models`

Model discovery, download metadata, local lifecycle and recommendation inputs. It must not silently download or execute untrusted artifacts.

### `platform`

Capability discovery and operating-system-specific adapters for shortcuts, clipboard, text insertion, target application identification and permissions.

### `storage`

Local history, settings and migrations. Raw audio retention is off by default.

### `diagnostics`

Structured checks for microphone, model runtime, acceleration backend, shortcut registration, clipboard, insertion and integrations.

### `integrations`

Permission-scoped bridges for external applications and agents. MCP is expected to be one protocol, not the entire integration architecture.

### `benchmark`

Reproducible latency, real-time factor, memory and later accuracy/calibration measurements.

## 4. Data and privacy boundaries

BLCVoice is designed around local ownership of dictation data.

Initial defaults:

- microphone audio is processed ephemerally and not retained after the dictation operation,
- transcription history is local,
- network providers are opt-in,
- secrets must not be stored as plaintext application settings,
- integrations receive only explicitly granted capabilities,
- telemetry is not assumed by the architecture and must be introduced only by a separate decision.

## 5. Concurrency and cancellation

Long-running work such as audio capture, model loading, transcription and downloads must be cancellable. The architecture should avoid a UI thread owning inference lifecycle.

A dictation session should have a unique identifier and a bounded state machine so stale results cannot be inserted into a newly focused application accidentally.

Conceptual states:

```text
idle
-> arming
-> recording
-> finalizing_audio
-> transcribing
-> transforming
-> inserting
-> completed | failed | cancelled
```

## 6. Failure model

Expected failures include:

- microphone disappears or permission is revoked,
- no speech is detected,
- model files are missing/corrupt,
- requested hardware backend is unavailable,
- GPU memory is insufficient,
- shortcut registration conflicts,
- focused target changes during a session,
- clipboard/insertion is denied,
- integration process disconnects,
- persistence is unavailable.

Failures should be typed and actionable rather than collapsed into generic strings.

## 7. Compatibility policy

Compatibility is evidence-based. A platform or environment is considered supported only after its critical dictation path is covered by repeatable testing. Environments may be marked experimental when automated or hardware-backed coverage is incomplete.

## 8. What is deliberately outside the initial architecture

The first implementation will not optimize for:

- meeting recording,
- speaker diarization,
- autonomous agents,
- account/cloud synchronization,
- plugin marketplaces,
- semantic-history search,
- mobile clients.

These may be added later only if the core product proves useful and the architecture can absorb them without weakening reliability.

## 9. Decision records

Architecture decisions that materially change these boundaries must be recorded under [`docs/adr/`](docs/adr/).
