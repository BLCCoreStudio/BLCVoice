# BLCVoice

**Fast, private, cross-platform voice dictation that works where you type.**

BLCVoice is an early-stage open-source desktop dictation project focused on a simple interaction: press a shortcut, speak naturally, and place accurate text into the application you are already using.

## Status

BLCVoice is in **pre-alpha development**. The architecture and reliability requirements are being established before feature implementation begins. There are no production-ready releases yet.

## Product principles

- **Useful before expansive.** The core dictation loop must be reliable before broader AI features are added.
- **Local-first.** Local speech recognition is a first-class path; optional remote providers may be added behind explicit user choice.
- **Engine-agnostic.** The application core must not be coupled to a single speech-recognition model or runtime.
- **Cross-platform by capability.** Windows, Linux/X11 and Linux/Wayland are treated as distinct capability environments rather than hidden behind one generic platform flag.
- **Minimal permissions.** Microphone, text insertion, integrations, history and future agent access are scoped independently.
- **Measured reliability.** Capturing audio, transcription and text delivery are separate pipeline stages; an attempted insertion is not treated as a confirmed delivery.

## Initial scope

The first usable milestone is intentionally narrow:

1. global push-to-talk,
2. microphone selection and reliable audio capture,
3. local speech recognition,
4. automatic model/backend recommendation,
5. voice activity detection,
6. reliable text insertion,
7. a lightweight overlay,
8. basic local history,
9. diagnostics and actionable failure reporting.

Deep integrations with tools such as Claude Code, Codex and VS Code are planned only after the universal dictation path is dependable.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for the current system boundaries and [docs/adr](docs/adr) for accepted architecture decisions.

## Contributing

The project is not yet accepting large feature implementations while the core interfaces are being stabilized. Bug reports, design feedback and focused proposals are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Security

Please do not report security-sensitive issues publicly. See [SECURITY.md](SECURITY.md).

## License

BLCVoice is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)), or
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
