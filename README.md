# BLCVoice

**Fast, private, cross-platform voice dictation that works where you type.**

BLCVoice is an early-stage open-source desktop dictation project focused on a simple interaction: press a shortcut, speak naturally, and place accurate text into the application you are already using.

## Status

BLCVoice is in **pre-alpha development**. The repository now contains the Rust/Tauri desktop foundation, a bounded dictation lifecycle, cross-platform microphone discovery/capture adapters, audio preprocessing, engine-agnostic ASR contracts, a transcribe.cpp adapter, runtime-level capture-to-ASR orchestration, a native microphone capture bridge and a usable microphone-selection/test screen. Runtime-independent global-shortcut semantics are defined, and the desktop host routes shortcut registration through native global-hotkey support on Windows/macOS/X11 or the XDG GlobalShortcuts portal on Wayland. The registered shortcut currently emits typed start/stop decisions; model management, shortcut-driven ASR, VAD, text insertion and a production-ready dictation UI are not implemented yet, and there are no production-ready releases.

## Product principles

- **Useful before expansive.** The core dictation loop must be reliable before broader AI features are added.
- **Local-first.** Local speech recognition is a first-class path; optional remote providers may be added behind explicit user choice.
- **Engine-agnostic.** The application core must not be coupled to a single speech-recognition model or runtime.
- **Cross-platform by capability.** Windows, Linux/X11 and Linux/Wayland are treated as distinct capability environments rather than hidden behind one generic platform flag.
- **Minimal permissions.** Microphone, text insertion, integrations, history and future agent access are scoped independently.
- **Measured reliability.** Capturing audio, transcription and text delivery are separate pipeline stages; an attempted insertion is not treated as a confirmed delivery.

## Initial scope

The first usable milestone is intentionally narrow:

1. global toggle-to-talk by default, with optional push-to-talk,
2. microphone selection and reliable audio capture,
3. local speech recognition,
4. automatic model/backend recommendation,
5. voice activity detection,
6. reliable text insertion,
7. a lightweight overlay,
8. basic local history,
9. diagnostics and actionable failure reporting.

The default planned interaction is `Ctrl+Shift+Space`: press once to start dictation and press again to stop. Push-to-talk remains an optional mode for short utterances. Deep integrations with tools such as Claude Code, Codex and VS Code are planned only after the universal dictation path is dependable.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the current system boundaries and [docs/adr](docs/adr) for accepted architecture decisions.

## Development

The current foundation pins Rust 1.98.0. On Linux, install the Tauri 2 and native audio system prerequisites for your distribution before building the desktop shell.

```bash
cargo test -p blcvoice-core -p blcvoice-runtime -p blcvoice-shortcuts --all-targets
cargo run -p blcvoice-desktop
```

CI validates the runtime-independent core on Linux, Windows and macOS, validates native audio and ASR adapters on all three platforms, checks the static desktop JavaScript/configuration, and tests/lints the desktop shell on Linux, Windows and macOS.

## Contributing

The project is not yet accepting large feature implementations while the core interfaces are being stabilized. Bug reports, design feedback and focused proposals are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

Please do not report security-sensitive issues publicly. See [SECURITY.md](SECURITY.md).

## License

BLCVoice is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
