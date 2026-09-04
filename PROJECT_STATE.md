# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-05 against `main` at `7fc1d50714b81c913cd229fbfd0a8005fa6925e9` plus live GitHub PR/issue state.

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
- deterministic preprocessing, `transcribe.cpp` cold/warm ASR and core post-stop benchmark probes from PRs #48-#50;
- the canonical autonomous-development/source-of-truth operating layer;
- reconciled maintainer ownership, support/conduct, release-note, RustSec and patch-only Dependabot automation from PR #46.

Compatibility claims remain evidence-based. Compile/lint/unit/package coverage is not equivalent to real desktop-session validation.

## Recently completed

- **PR #41 — `build: add cross-platform desktop bundle pipeline`** merged as `5e72ebd6d1c771f052c8b6ec34f5def97986d07d` after normal CI and the revised bundle matrix passed on the unchanged head SHA.
- **PR #46 — `chore: refresh maintainer automation baseline`** merged as `00c59bdfe0a01c7dcce552130325938fa497e9ac`; it supersedes stale PR #17.
- **PR #47 — `ci: add fail-closed critical validation gate`** merged as `ba54d262dca84a02f28b61c166ea19da8e4f6e0f` after the relevant checks passed on its unchanged head SHA.
- **PR #48 — `perf: add reproducible preprocessing benchmark foundation`** merged as `41a6fdcbb303296d550f0400b35fb4e1acaf6932` after CI, Security Audit and Desktop bundles passed on unchanged head `27ca6538a43b3493ff7e07ee67b1b4f30ce18398`.
- **PR #49 — `perf: add transcribe ASR cold/warm benchmark`** merged as `21fd32218fc40a41dc5e50cbee85dd2e6f32892c` after CI, Security Audit and Desktop bundles passed on unchanged head `d2330b9e7ed2c603a5bbadf94c666ca3fb19187d`.
- **PR #50 — `perf: add deterministic post-stop dictation benchmark`** merged as `7fc1d50714b81c913cd229fbfd0a8005fa6925e9` after the relevant checks passed on its unchanged head SHA.
- AppImage remains intentionally deferred. Re-entry requires a verified Wayland-safe released Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence without silent XWayland fallback.
- Production signing/notarization remains outside the autonomous trust boundary.

## Active work

- **#42 external governance follow-up** — repository-side CI gate work is merged. The live `main-protection` ruleset still requires an administration-capable settings update to require `Critical validation gate`; this remains documented in `docs/ci-required-checks.md` and must not be misrepresented as complete.
- **#43 benchmark foundation / PR #52** — active. The branch combines the final benchmark-evidence slices: platform-qualified process-memory counters with native semantics plus an engine-neutral corpus WER scorer requiring explicit aligned reference/hypothesis transcripts. Synthetic generated inference input remains excluded from accuracy evidence. CI and Security Audit are green on exact head `bcfffc3edc1064afe442b81b8786cf1b77df6c8d`; desktop bundle validation is still completing before merge.
- **#44 real-platform validation preparation** — stacked work has started from the exact #52 head so it cannot bypass #43. `docs/platform-validation.md` is the new canonical runtime-evidence matrix and replaces the narrower Wayland-only truth source. `docs/platform-validation-evidence-template.md` defines per-environment evidence records and explicitly separates backend/protocol acceptance from semantic target-document verification.

## Canonical backlog

Longer-lived work is tracked in GitHub Issues, not a duplicate `ROADMAP.md`.

- **#42** — align protected-branch required checks with the critical validation matrix; repository-side work is merged, live ruleset administration remains external.
- **#43** — add a reproducible core dictation benchmark harness.
- **#44** — establish the real-platform compatibility validation matrix.

Create additional issues only after checking for overlapping PRs/issues and accepted scope/ADR constraints.

## Known validation and governance gaps

- The live protected-branch ruleset does not yet require `Critical validation gate`; the repository-side check exists and the exact administration action is documented in `docs/ci-required-checks.md`.
- Real Windows/macOS/X11/KDE Wayland/GNOME Wayland end-to-end compatibility evidence is incomplete; issue #44 owns the cross-platform validation matrix.
- PR #52 must still complete the final desktop bundle validation on its unchanged head before #43 can merge and close.
- The #44 repository-side matrix is being prepared, but real desktop-session rows remain `UNVALIDATED` or `PARTIAL` until executed on representative sessions.
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.

## Next safe task

**Finish #43 on PR #52, then promote the stacked #44 validation work onto `main`.**

Exact next action:

1. Merge PR #52 only after CI, Security Audit and every relevant desktop bundle job are green on unchanged head `bcfffc3edc1064afe442b81b8786cf1b77df6c8d` and review threads remain clear.
2. Close #43 if its acceptance criteria still hold after merge.
3. Rebase/retarget the stacked #44 validation branch onto the merged `main` history without dropping #52 evidence.
4. Validate the canonical matrix/evidence-template changes, then merge them before collecting runtime evidence.
5. Execute every environment row available through current tooling; record unavailable physical/session validation as `BLOCKED_EXTERNAL`, never as passing.
6. Update support claims only after the relevant row is `VALIDATED` on an exact commit.
7. Independently, add `Critical validation gate` to the active `main-protection` ruleset when administration access is available, then validate the live ruleset and close #42.
8. Do **not** configure production signing/notarization credentials or publish a production release.

## Mandatory external gates

Autonomous work stops before:

- production signing/notarization credential handling;
- production release-account, app-store/account operations requiring external account authority;
- secret/credential creation, entry, rotation or disclosure outside an already-authorized safe mechanism;
- paid or financially binding irreversible external-service commitments;
- legal/account-level terms or agreement acceptance on the user's behalf;
- production publication when it crosses one of those external trust/account/legal boundaries.

Unsigned/draft artifacts, local validation, documentation and preparatory code may proceed up to those gates.

## Update rule

Every substantive PR that changes implemented capability, active work, validation status, backlog ordering or the exact next-safe-task pointer must update this file in the same PR. Do not copy ADR rationale or detailed architecture into this file.
