# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-04 against `main` at `7fc1d50714b81c913cd229fbfd0a8005fa6925e9` plus live GitHub PR/issue state.

## Development stage

BLCVoice is pre-alpha. The priority remains a reliable local-first universal dictation loop before broader AI/integration features.

## Current implementation state

`main` currently contains the Rust/Tauri desktop foundation and production dictation path with:

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
- a fail-closed aggregate critical CI validation gate;
- deterministic preprocessing, `transcribe.cpp` cold/warm ASR and engine-neutral post-stop benchmark probes;
- the canonical autonomous-development/source-of-truth operating layer;
- maintainer ownership, support/conduct, release-note, RustSec and patch-only Dependabot automation.

Compatibility claims remain evidence-based. Compile/lint/unit/package coverage is not equivalent to real desktop-session validation.

## Recently completed

- **PR #47 — `ci: add fail-closed critical validation gate`** merged as `ba54d262dca84a02f28b61c166ea19da8e4f6e0f`.
- **PR #48 — `perf: add reproducible preprocessing benchmark foundation`** merged as `41a6fdcbb303296d550f0400b35fb4e1acaf6932` after CI, Security Audit and Desktop bundles passed on unchanged head.
- **PR #49 — `perf: add transcribe ASR cold/warm benchmark`** merged as `21fd32218fc40a41dc5e50cbee85dd2e6f32892c` after CI, Security Audit and Desktop bundles passed on unchanged head.
- **PR #50 — `perf: add deterministic post-stop dictation benchmark`** merged as `7fc1d50714b81c913cd229fbfd0a8005fa6925e9` after CI, Security Audit and Desktop bundles passed on unchanged head `71e8ab9fae52750b88e03dc9f13fcc6fb0eb0162`.
- AppImage remains intentionally deferred. Re-entry requires a verified Wayland-safe released Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence without silent XWayland fallback.
- Production signing/notarization remains outside the autonomous trust boundary.

## Active work

- **#42 external governance follow-up** — repository-side CI gate work is merged. The live `main-protection` ruleset still requires an administration-capable settings update to require `Critical validation gate`; this remains documented in `docs/ci-required-checks.md` and must not be misrepresented as complete.
- **#43 benchmark foundation** — active. Timing probes are merged. The current slice adds platform-qualified peak-memory evidence wrappers plus an engine-neutral WER/CER scorer. Native resource semantics remain distinct by OS, deterministic CI validates tooling only, and no hosted-runner resource number or scorer self-test is a product performance/accuracy claim.

## Canonical backlog

Longer-lived work is tracked in GitHub Issues, not a duplicate `ROADMAP.md`.

- **#42** — align protected-branch required checks with the critical validation matrix; repository-side work is merged, live ruleset administration remains external.
- **#43** — add a reproducible core dictation benchmark harness.
- **#44** — establish the real-platform compatibility validation matrix.

Create additional issues only after checking for overlapping PRs/issues and accepted scope/ADR constraints.

## Known validation and governance gaps

- The live protected-branch ruleset does not yet require `Critical validation gate`; the repository-side check exists and the exact administration action is documented in `docs/ci-required-checks.md`.
- Real Windows/macOS/X11/KDE Wayland/GNOME Wayland end-to-end compatibility evidence is incomplete; issue #44 owns the cross-platform validation matrix.
- A real-model accuracy result still requires a provenance-qualified model and licensed reference corpus; deterministic scorer validation alone must not be reported as product accuracy.
- Real-model resource evidence is environment-specific; hosted-runner smoke values must not become release baselines.
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.

## Next safe task

**Finish issue #43, then continue directly with #44 while keeping #42's live-ruleset administration gap explicit.**

Exact next action:

1. Validate the platform-qualified resource wrappers and accuracy scorer with deterministic cross-platform CI on the unchanged PR head.
2. Merge only when formatting/tests/Clippy, Security Audit, Desktop bundles and all critical CI checks are green, review threads are clear and the head SHA is unchanged.
3. Close #43 when its benchmark-foundation acceptance criteria are met; do not invent a real-model accuracy baseline if no provenance-qualified model/corpus evidence was executed.
4. Continue immediately with #44 and consolidate the real-platform validation matrix into `docs/wayland-eis-validation.md` plus the other required platform rows.
5. Independently, add `Critical validation gate` to the active `main-protection` ruleset when administration access is available, then validate the live ruleset and close #42.
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
