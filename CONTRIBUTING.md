# Contributing to BLCVoice

BLCVoice is in early development. Contributions are welcome, but the project is deliberately stabilizing its core boundaries before accepting large feature additions.

## Before opening a pull request

- Check existing issues and pull requests for overlapping work.
- Keep changes focused and independently reviewable.
- Do not introduce a new speech engine, platform abstraction, database, UI framework or integration protocol without an architecture discussion first.
- Do not claim support for an operating system, desktop environment or feature that has not been exercised by tests or explicitly marked experimental.
- Do not add telemetry, network access or persistence of microphone/audio data without an explicit design decision and privacy review.

## Development principles

1. **Reliability before feature count.** The core dictation path takes priority over adjacent AI features.
2. **Capability-based platform code.** Platform-specific behavior must remain behind explicit interfaces.
3. **Engine independence.** Speech-recognition runtimes are adapters, not the application architecture.
4. **Least privilege.** New capabilities should request the smallest permission surface possible.
5. **Measured claims.** Performance, accuracy and compatibility statements should be backed by reproducible evidence.
6. **Clear failures.** A failed insertion, unavailable backend or denied permission must not be represented as success.

## Pull requests

Pull requests should explain:

- what problem is being solved,
- why the chosen approach fits the architecture,
- what was tested,
- platform-specific effects,
- security/privacy implications when relevant.

Prefer small pull requests. Large refactors should be split into reviewable stages where possible.

## Commit and merge policy

Feature work should be developed on branches and merged through pull requests. The repository is intended to use squash merges on `main` so each merged pull request represents one logical change.

## Architecture decisions

Significant choices should be captured as Architecture Decision Records under `docs/adr/`.

An ADR is appropriate when a decision affects areas such as:

- core crate/module boundaries,
- platform integration strategy,
- ASR engine contracts,
- persistence,
- security boundaries,
- external integration protocols,
- compatibility policy.

## Licensing contributions

Unless explicitly stated otherwise, contributions are accepted under the repository's dual license: **MIT OR Apache-2.0**, at the recipient's option.
