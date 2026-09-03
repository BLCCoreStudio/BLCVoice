# BLCVoice

**Fast, private, cross-platform voice dictation that works where you type.**

BLCVoice is an early-stage open-source desktop dictation project focused on a simple interaction: press a shortcut, speak naturally, and place accurate text into the application you are already using.

## Status

BLCVoice is in **pre-alpha development**. The repository contains the Rust/Tauri desktop foundation, a bounded dictation lifecycle, cross-platform microphone discovery/capture adapters, audio preprocessing, engine-agnostic ASR contracts, a transcribe.cpp adapter, runtime-level capture-to-ASR orchestration, native microphone capture and a microphone-selection/test screen. Global-shortcut registration is capability-driven: Windows/macOS and Linux/X11 use the native global-hotkey path, while Linux/Wayland uses the compositor-mediated XDG GlobalShortcuts portal.

The production desktop dictation command path now reaches a real terminal lifecycle: it prepares the local recognizer before recording, captures and finalizes audio, preprocesses it to the recognizer format, transcribes it, selects a platform insertion backend, submits the complete transcript and commits `Completed` only after the backend accepts the full submission. Windows/macOS use a native Unicode insertion adapter, Linux/X11 uses XTEST with a temporary restored Unicode keysym mapping, and Linux/Wayland uses XDG RemoteDesktop + EIS `ei_text` without root/raw-input fallbacks. Insertion failures are recorded as `TextInsertion` failures and return the transcript as recoverable text. A successful backend receipt still does **not** claim that an arbitrary target application's document mutation is semantically observable.

The global shortcut is not yet connected to the configured production dictation path, and automatic model management, VAD, persistent production settings, local history, the lightweight dictation overlay and release packaging are still incomplete. Platform adapters are continuously compiled/linted in CI; real application compatibility remains a separate runtime-validation requirement.

## Product principles

- **Useful before expansive.** The core dictation loop must be reliable before broader AI features are added.
- **Local-first.** Local speech recognition is a first-class path; optional remote providers may be added behind explicit user choice.
- **Engine-agnostic.** The application core must not be coupled to a single speech-recognition model or runtime.
- **Cross-platform by capability.** Windows, Linux/X11 and Linux/Wayland are treated as distinct capability environments rather than hidden behind one generic platform flag.
- **Minimal permissions.** Microphone, text insertion, integrations, history and future agent access are scoped independently.
- **Measured reliability.** Capturing audio, transcription and text delivery are separate pipeline stages; an attempted insertion is not treated as confirmed semantic delivery.

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
cargo test -p blcvoice-core -p blcvoice-runtime -p blcvoice-platform -p blcvoice-shortcuts -p blcvoice-insertion -p blcvoice-insertion-eis -p blcvoice-insertion-x11 -p blcvoice-insertion-native --all-targets
cargo run -p blcvoice-desktop
```

CI validates the runtime-independent core and insertion contracts on Linux, Windows and macOS, builds/lints the Wayland EIS, X11 XTEST and native Windows/macOS insertion adapters through their cross-platform crate boundaries, validates native audio and ASR adapters on all three platforms, checks the static desktop JavaScript/configuration, and tests/lints the desktop shell on Linux, Windows and macOS. Linux also runs the X11 adapter against a live Xvfb/XTEST server.

## Contributing

The project is not yet accepting large feature implementations while the core interfaces are being stabilized. Bug reports, design feedback and focused proposals are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

Please do not report security-sensitive issues publicly. See [SECURITY.md](SECURITY.md).

## License

BLCVoice is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
