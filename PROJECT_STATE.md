# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-03 against `main` at `64f9bcc6e4b634aa72ab7c21adf64747f2bd8155` plus live GitHub PR/issue state.

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
- the canonical autonomous-development/source-of-truth operating layer.

Compatibility claims remain evidence-based. Compile/lint/unit coverage is not equivalent to real desktop-session validation.

## Active pull requests

1. **PR #41 — `build: add cross-platform desktop bundle pipeline`**
   - Current priority.
   - The previous CI run `33790324156` ended `action_required` before creating jobs. Its actor and triggering actor were `github-actions[bot]`, matching GitHub's recursion-protection behavior for workflow-triggered pull-request activity using `GITHUB_TOKEN`; this was not an application test failure.
   - A direct branch update restored normal runnable CI and started the dedicated bundle-validation workflow.
   - macOS arm64 `.app` and `.dmg` artifacts were successfully produced in bundle run `33798107152`.
   - The first Linux bundle attempt on Ubuntu 22.04 failed in `libspa`: BLCVoice enables CPAL 0.18.2's native PipeWire backend, which requires PipeWire 0.3.53+, while Ubuntu 22.04 supplied 0.3.48 development headers.
   - Linux packaging is being moved to a Debian 12 container. Debian 12 supplies PipeWire 0.3.65 while remaining a Tauri-recommended WebKitGTK/AppImage compatibility baseline.
   - Packaging policy: Linux x64 `.deb`/`.AppImage` in Debian 12, Windows x64 NSIS on Windows Server 2025, macOS arm64/x64 `.app`/`.dmg` on explicit macOS 15 runners.
   - macOS validation/draft artifacts use ad-hoc signing only. Production signing/notarization remains outside the autonomous trust boundary.
   - ADR 0026 records the material packaging/release trust-boundary decision and the evidence behind the Debian 12 baseline.

2. **PR #17 — `chore: establish maintainer automation baseline`**
   - Adds CODEOWNERS/support/conduct/security-audit/release-note maintenance infrastructure.
   - It predates substantial later `main` work and must be reconciled with current `main` before merge rather than assumed current.

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

## Next safe task

**Finish PR #41 before starting another product feature.**

Exact next action:

1. Validate the revised Debian 12 Linux packaging job and require `.deb` plus `.AppImage` workflow artifacts.
2. Require Windows x64 NSIS, macOS arm64 `.app`/`.dmg`, and macOS x64 `.app`/`.dmg` artifacts from the same current PR head.
3. Require every applicable critical normal-CI job to pass on the current PR head.
4. Inspect any packaging/architecture-specific failure logs and repair only with current upstream evidence.
5. Confirm release-write permission remains confined to tag-triggered draft-release jobs.
6. Merge PR #41 only when normal CI, bundle validation, review-thread and architecture/ADR documentation gates are green.
7. Do **not** configure production signing/notarization credentials or publish a production release.

After #41 is safely merged, reconcile `PROJECT_STATE.md` to remove it from active work and make PR #17 the next safe task if it is still applicable. Then continue through issues #42, #43 and #44 by current risk/blocking value, with broken-main/security/regression work taking precedence if it appears.

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
