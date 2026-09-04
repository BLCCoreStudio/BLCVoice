# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-04 against `main` at `ba54d262dca84a02f28b61c166ea19da8e4f6e0f` plus live GitHub PR/issue state.

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
- a fail-closed aggregate critical CI validation gate from PR #47;
- the canonical autonomous-development/source-of-truth operating layer;
- reconciled maintainer ownership, support/conduct, release-note, RustSec and patch-only Dependabot automation from PR #46.

Compatibility claims remain evidence-based. Compile/lint/unit/package coverage is not equivalent to real desktop-session validation.

## Recently completed

- **PR #41 — `build: add cross-platform desktop bundle pipeline`** merged as `5e72ebd6d1c771f052c8b6ec34f5def97986d07d` after normal CI and the revised bundle matrix passed on the unchanged head SHA.
- **PR #46 — `chore: refresh maintainer automation baseline`** merged as `00c59bdfe0a01c7dcce552130325938fa497e9ac`; it supersedes stale PR #17.
- **PR #47 — `ci: add fail-closed critical validation gate`** merged as `ba54d262dca84a02f28b61c166ea19da8e4f6e0f` after the relevant checks passed on its unchanged head SHA.
- AppImage remains intentionally deferred. Re-entry requires a verified Wayland-safe released Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence without silent XWayland fallback.
- Production signing/notarization remains outside the autonomous trust boundary.

## Active work

- **#42 external governance follow-up** — repository-side CI gate work is merged. The live `main-protection` ruleset still requires an administration-capable settings update to require `Critical validation gate`; this remains documented in `docs/ci-required-checks.md` and must not be misrepresented as complete.
- **#43 benchmark foundation** — active. The first slice adds a deterministic engine-neutral audio-preprocessing benchmark with cold/warm timing, RTF and environment metadata plus an evidence contract in `docs/benchmarking.md`.

## Canonical backlog

Longer-lived work is tracked in GitHub Issues, not a duplicate `ROADMAP.md`.

- **#42** — align protected-branch required checks with the critical validation matrix; repository-side work is merged, live ruleset administration remains external.
- **#43** — add a reproducible core dictation benchmark harness.
- **#44** — establish the real-platform compatibility validation matrix.

Create additional issues only after checking for overlapping PRs/issues and accepted scope/ADR constraints.

## Known validation and governance gaps

- The live protected-branch ruleset does not yet require `Critical validation gate`; the repository-side check exists and the exact administration action is documented in `docs/ci-required-checks.md`.
- Real Windows/macOS/X11/KDE Wayland/GNOME Wayland end-to-end compatibility evidence is incomplete; issue #44 owns the cross-platform validation matrix.
- Reproducible model-load, ASR inference, end-to-end post-stop latency, resource-use and later accuracy evidence is not yet complete; issue #43 owns that foundation.
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.

## Next safe task

**Continue issue #43 while keeping #42's live-ruleset administration gap explicit.**

Exact next action:

1. Validate the deterministic preprocessing benchmark on its exact PR head SHA with formatting, tests, Clippy and the cross-platform CI matrix.
2. Keep CI timing results informational; do not establish performance thresholds from hosted-runner timing.
3. Merge only when all relevant checks are green, review threads are clear and the head SHA is unchanged.
4. Continue #43 with engine-adapter cold/warm model-load and inference measurements plus post-stop-to-transcript timing without leaking engine-specific policy into core contracts.
5. Continue with #44 after the benchmark foundation is stable.
6. Independently, add `Critical validation gate` to the active `main-protection` ruleset when administration access is available, then validate the live ruleset and close #42.
7. Do **not** configure production signing/notarization credentials or publish a production release.

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
