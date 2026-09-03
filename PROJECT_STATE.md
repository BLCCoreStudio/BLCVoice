# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-03 against `main` at `a2470984fd7aae84eccb80729fa419c38dcaae82` plus live GitHub PR/issue state.

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
- engine-agnostic Silero VAD integrated into production dictation.

Compatibility claims remain evidence-based. Compile/lint/unit coverage is not equivalent to real desktop-session validation.

## Active pull requests

1. **PR #41 — `build: add cross-platform desktop bundle pipeline`**
   - Adds Linux, Windows and macOS bundle generation plus draft prerelease mechanics.
   - Signing/notarization stays outside the repository trust boundary.
   - Current head: `170280cb2d8dabc6503eb135020fbfc74b42be0d`.
   - Current PR CI run `33790324156` concluded `action_required`; this must be resolved before the PR can count as complete.

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
- `README.md` previously duplicated fast-moving implementation status and had drifted behind `main`; it should now point here for current state instead of becoming a second state tracker.

## Next safe task

**Finish PR #41 before starting another product feature.**

Exact next action:

1. Diagnose why PR #41 CI run `33790324156` is `action_required`.
2. Restore a normal runnable CI path and make every applicable critical check pass.
3. Validate the bundle pipeline produces the intended Linux `.deb`/`.AppImage`, Windows NSIS and macOS `.app`/`.dmg` artifacts without claiming signing/notarization.
4. Review packaging failure modes and trust-boundary wording.
5. Merge only when repository/review/critical-CI gates pass.
6. Do **not** configure production signing/notarization credentials or publish a production release; those are mandatory stop gates.

After #41 is safely completed, reconcile and finish PR #17 if it is still applicable. Then select from issues #42, #43 and #44 by current risk/blocking value, with broken-main/security/regression work taking precedence if it appears.

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
