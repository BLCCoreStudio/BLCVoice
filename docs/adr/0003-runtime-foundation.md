# ADR 0003: Runtime foundation

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice needs an executable foundation without prematurely coupling the product to a speech-recognition runtime, JavaScript dependency graph, operating-system input strategy or agent integration.

The first implementation must prove that the platform-independent dictation lifecycle can live outside the desktop UI and that the native application shell can consume that core through a narrow boundary.

## Decision

1. The platform-independent product core is a Rust workspace crate named `blcvoice-core`.
2. The desktop application is a Tauri 2 Rust crate named `blcvoice-desktop`.
3. Rust 1.98.0 is the pinned project toolchain and new project code uses the Rust 2024 edition.
4. The initial core has no third-party runtime dependency. It owns a bounded dictation-session state machine and typed failure stages only.
5. The initial Tauri shell uses a static local HTML/CSS surface. React and TypeScript remain the intended product UI stack, but they will be introduced with their own lockfile, build checks and UI test boundary rather than being mixed into the native bootstrap.
6. No ASR, audio, VAD, clipboard, text-insertion or agent SDK is selected in this decision.
7. The Tauri capability set is kept minimal and the initial UI loads no remote content.

## Rationale

This order keeps the first code review small enough to validate the architecture itself. It avoids treating a framework scaffold as product progress and keeps future engine/platform decisions replaceable.

It also gives CI something concrete to verify immediately: the core state machine on Linux, Windows and macOS, plus a native Tauri compilation check on Linux.

## Consequences

- The repository has a real executable desktop foundation before dictation features begin.
- React/TypeScript is intentionally deferred to a dedicated UI foundation change.
- A committed `Cargo.lock` is required once CI has resolved and validated the initial Tauri dependency graph.
- Future audio, ASR and insertion work must depend on the core contract rather than placing lifecycle state directly in the UI or platform adapters.
