# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-04 against `main` at `5e72ebd6d1c771f052c8b6ec34f5def97986d07d` plus live GitHub PR/issue state.

## Development stage

BLCVoice is pre-alpha. The priority remains a reliable local-first universal dictation loop before broader AI/integration features.

## Current implementation state

`main` currently contains the Rust/Tauri desktop foundation and the production dictation path with:

- bounded session/runtime orchestration;
- native microphone discovery/capture and preprocessing;
- engine-agnostic ASR contracts with the first `transcribe.cpp` adapter;
- persistent settings and local model management;
- capability-driven global-shortcut handling;
- capability-driven Windows/macOS, Linux/X11 and Linux/Wayland text-insertion adapters;
- production shortcut-to-dictation wiring;
- lightweight dictation overlay and tray-resident behavior;
- engine-agnostic Silero VAD integrated into production dictation;
- cross-platform desktop bundle validation for Linux x64 `.deb`, Windows x64 NSIS, and macOS arm64/x64 `.app` + `.dmg`;
- the canonical autonomous-development/source-of-truth operating layer.

Compatibility claims remain evidence-based. Compile/lint/unit/package coverage is not equivalent to real desktop-session validation.

## Recently completed

- **PR #41 — `build: add cross-platform desktop bundle pipeline`** merged as `5e72ebd6d1c771f052c8b6ec34f5def97986d07d` after normal CI and the revised bundle matrix passed on the unchanged head SHA.
- AppImage remains intentionally deferred. Re-entry requires a verified Wayland-safe released Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence without silent XWayland fallback.
- Production signing/notarization remains outside the autonomous trust boundary.

## Active pull requests

1. **PR #46 — `chore: refresh maintainer automation baseline`**
   - Current priority.
   - Rebuilds the useful scope of stale PR #17 from current `main` rather than merging its old base forward.
   - Adds CODEOWNERS, support/conduct policy, generated release-note categories, patch-only Dependabot auto-merge, and a scheduled RustSec audit.
   - Must pass current CI on its exact head before merge.

2. **PR #17 — `chore: establish maintainer automation baseline`**
   - Superseded by PR #46 once #46 contains the reconciled source-of-truth update and validates cleanly.
   - Do not merge #17 independently.

## Canonical backlog

Longer-lived work is tracked in GitHub Issues, not a duplicate `ROADMAP.md`.

- **#42** — align protected-branch required checks with the critical validation matrix.
- **#43** — add a reproducible core dictation benchmark harness.
- **#44** — establish the real-platform compatibility validation matrix.

Create additional issues only after checking for overlapping PRs/issues and accepted scope/ADR constraints.

## Known validation and governance gaps

- The protected-branch required-check list does not yet cover every critical CI job; issue #42 owns the reconciliation.
- Real Windows/macOS/X11/KDE Wayland/GNOME Wayland end-to-end compatibility evidence is incomplete; issue #44 owns the cross-platform validation matrix.
- Reproducible model/runtime latency, real-time factor, resource-use and later accuracy evidence is not yet a canonical benchmark system; issue #43 owns that foundation.
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.

## Next safe task

**Finish PR #46 before starting another product feature.**

Exact next action:

1. Validate the new maintainer automation files against current `main` and current GitHub guidance.
2. Require every applicable critical CI job to pass on the unchanged PR #46 head SHA.
3. Inspect review threads and merge PR #46 only when its relevant checks are green.
4. Close superseded PR #17 after #46 is safely merged.
5. Then continue with issue #42 because merge-protection correctness is the highest remaining governance risk before benchmark and compatibility-matrix work.
6. Do **not** configure production signing/notarization credentials or publish a production release.

## Mandatory external gates

Autonomous work stops before:

- production signing/notarization credential handling;
- production release/store/account operations requiring external account authority;
- secret/credential creation, entry, rotation or disclosure outside an already-authorized safe mechanism;
- paid or financially binding irreversible external-service commitments;
- legal/account-level terms or agreement acceptance on the user's behalf;
- production publication when it crosses one of those external trust/account/legal boundaries.

Unsigned/draft artifacts, local validation, documentation and preparatory code may proceed up to those gates.

## Update rule

Every substantive PR that changes implemented capability, active work, validation status, backlog ordering or the exact next-safe-task pointer must update this file in the same PR. Do not copy ADR rationale or detailed architecture into this file.
