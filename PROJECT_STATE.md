# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-04 against `main` at `64f9bcc6e4b634aa72ab7c21adf64747f2bd8155` plus live GitHub PR/issue state.

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
   - Normal CI is green at head `1430cc7a8ea91b2c5c53cf18725938fb9a6a116a`.
   - Bundle run `33845994202` produced Windows x64 NSIS, macOS arm64 `.app`/`.dmg`, macOS x64 `.app`/`.dmg`, and the Linux Debian 12 `.deb` artifact on that same head.
   - The Debian 12 production build requires `libclang-dev` because the PipeWire/SPA Rust bindings generate FFI bindings with bindgen.
   - AppImage alone still failed in the linuxdeploy stage. Current Tauri upstream evidence also shows a material Wayland trust concern: the 2.11-era AppImage GTK hook forced `GDK_BACKEND=x11`; upstream #15786 now preserves an explicit backend, but BLCVoice has not verified the released bundler path or runtime-validated the artifact on KDE Wayland.
   - Packaging policy is therefore narrowed to the proven matrix: Linux x64 `.deb` in Debian 12, Windows x64 NSIS on Windows Server 2025, macOS arm64/x64 `.app`/`.dmg` on explicit macOS 15 runners. AppImage is deferred behind explicit build + real-Wayland re-entry criteria in ADR 0026.
   - macOS validation/draft artifacts use ad-hoc signing only. Production signing/notarization remains outside the autonomous trust boundary.

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
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.

## Next safe task

**Finish PR #41 before starting another product feature.**

Exact next action:

1. Require Linux x64 `.deb`, Windows x64 NSIS, macOS arm64 `.app`/`.dmg`, and macOS x64 `.app`/`.dmg` artifacts from the same current PR head.
2. Require every applicable critical normal-CI job to pass on the current PR head.
3. Confirm AppImage is absent from both validation and tag-triggered draft-release paths and that ADR 0026/release documentation states its evidence-based re-entry criteria.
4. Confirm release-write permission remains confined to tag-triggered draft-release jobs.
5. Inspect review threads and merge PR #41 only when normal CI and the revised bundle-validation matrix are green on the unchanged head SHA.
6. Do **not** configure production signing/notarization credentials or publish a production release.

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
