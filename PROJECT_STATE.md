# BLCVoice Project State

This file is the canonical operational snapshot for autonomous development. It does not replace `ARCHITECTURE.md`, `docs/adr/`, GitHub Issues, pull requests, CI or repository rulesets.

Last reconciled: 2026-09-05 against `main` at `b483acf1c34a7db8af5ce5947345ca8aa5b27f5f` plus live GitHub PR/issue state.

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
- recognizer reuse keyed to model identity so successful sessions avoid unnecessary reloads;
- deterministic shortcut-to-dictation application-level E2E coverage at the production coordinator seam;
- truthful overlay copy that distinguishes insertion-backend acceptance from semantic target-document verification;
- cross-platform desktop bundle validation for Linux x64 `.deb`, Windows x64 NSIS, and macOS arm64/x64 `.app` + `.dmg`;
- a fail-closed aggregate critical CI validation gate from PR #47;
- deterministic preprocessing, `transcribe.cpp` cold/warm ASR, post-stop, platform-qualified process-memory and reproducible WER evidence tooling from PRs #48-#52;
- a canonical real-platform validation matrix and evidence template from PR #54;
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
- **PR #52 — `perf: complete platform-qualified benchmark evidence foundation`** merged as `454293877f784584ed5f86fa193aab9f4b565093` after CI, Security Audit and Desktop bundles passed on unchanged head `bcfffc3edc1064afe442b81b8786cf1b77df6c8d`. Issue #43 is closed as completed.
- **PR #54 — `test: establish canonical real-platform validation matrix`** merged as `7619dfdd125f2ece142abde32cc698e1a86f28db` after CI and Security Audit passed on unchanged head `45efd6d...`; issue #44 remains open because real desktop-session rows require runtime evidence.
- **PR #56 — `test: add deterministic shortcut-to-dictation application harness`** merged as `79a03c2ccd06ae1d77e87e157c78c0edadaa6002` after CI, Security Audit, Desktop bundles and the fail-closed critical validation gate passed on unchanged head `cc9f6a9747acf42bda7c5ccc5c1cba4132ca247d`. Issue #55 is closed through the merge.
- **PR #58 — `fix: keep overlay delivery semantics truthful`** merged as `b483acf1c34a7db8af5ce5947345ca8aa5b27f5f` after CI, Security Audit and all desktop bundle jobs passed on unchanged head `f8aaed828840aec03420c68449f89708fddb8408`. Issue #57 is closed through the merge.
- AppImage remains intentionally deferred. Re-entry requires a verified Wayland-safe released Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence without silent XWayland fallback.
- Production signing/notarization remains outside the autonomous trust boundary.

## Active work

- **#59 privacy-first local history storage** — active. ADR 0027 fixes the intended persistence boundary: app-private text-only SQLite through a Tauri-independent Rust storage service, transactional migrations/writes, no raw-audio retention, explicit deletion, and delivery metadata that never upgrades backend submission into semantic delivery. The implementation crate and desktop/history UI wiring remain to be completed in focused follow-up work.
- **#44 real-platform compatibility validation** — repository-side matrix is merged. Real Windows, macOS, Linux/X11, KDE Plasma 6 Wayland and GNOME Wayland semantic target-document evidence remains incomplete and must not be inferred from hosted runners, Xvfb or protocol acceptance.
- **#42 external governance follow-up** — repository-side CI gate work is merged. The live `main-protection` ruleset still requires an administration-capable settings update to require `Critical validation gate`; this remains documented in `docs/ci-required-checks.md` and must not be misrepresented as complete.

## Canonical backlog

Longer-lived work is tracked in GitHub Issues, not a duplicate `ROADMAP.md`.

- **#59** — implement the accepted local-history storage contract, then wire history into the production dictation application layer and expose a basic local history UI with explicit deletion.
- **#44** — execute the real-platform compatibility validation matrix on representative desktop sessions.
- **#42** — align the live protected-branch ruleset with the critical validation matrix; repository-side implementation is complete, ruleset administration remains external.

Create additional issues only after checking for overlapping PRs/issues and accepted scope/ADR constraints.

## Known validation and governance gaps

- The live protected-branch ruleset does not yet require `Critical validation gate`; the repository-side check exists and the exact administration action is documented in `docs/ci-required-checks.md`.
- Real Windows/macOS/X11/KDE Wayland/GNOME Wayland end-to-end compatibility evidence is incomplete; issue #44 owns the cross-platform validation matrix.
- Linux/X11 has a live Xvfb/XTEST smoke but not yet full shortcut-to-dictation semantic target-document validation.
- Real KDE Plasma 6 and GNOME Wayland EIS rows require representative desktop sessions; package/compile evidence is not substituted for those runtime rows.
- Basic local transcription history is not yet implemented. ADR 0027 now defines its persistence/data-retention boundary; #59 owns implementation.
- Diagnostics currently expose microphone, shortcut, insertion and recognizer status in the desktop UI but do not yet include history/persistence health; add it when the storage service is wired.
- AppImage is not a current deliverable. Re-entry requires a verified Wayland-safe Tauri bundler, green AppImage packaging, and real KDE Wayland runtime evidence.
- Production signing/notarization credentials are not available to autonomous repository tooling and are not required for repository-side RC preparation.

## Next safe task

**Validate and merge the ADR 0027/history-boundary documentation change, then implement #59's storage crate on current `main` before adding desktop wiring/UI.**

Exact next action:

1. Open the focused ADR/history-boundary PR and validate repository checks on one unchanged head SHA; merge only when applicable checks are green and review threads are clear.
2. Implement a dedicated `blcvoice-storage` crate using the accepted ADR 0027 contract, including schema versioning, transactional writes, typed errors, bounded list/delete operations and reopen/corruption/newer-schema tests.
3. Run formatting, storage/workspace tests, Clippy, RustSec and the cross-platform desktop bundle matrix with the bundled SQLite dependency before merge.
4. After the storage crate is green, wire successful transcriptions and insertion-failure recoverable text into history with truthful delivery state, add history IPC/UI and persistence-health diagnostics without letting persistence failures falsify dictation outcomes.
5. Execute any #44 runtime rows genuinely available through current tooling and save evidence tied to an exact commit; mark unavailable physical/session validation `BLOCKED_EXTERNAL` rather than passing.
6. Independently, add `Critical validation gate` to the active `main-protection` ruleset when administration access is available, then validate the live ruleset and close #42.
7. Do **not** configure production signing/notarization credentials or publish a production release.

## Mandatory external gates

Autonomous work stops before:

- production signing/notarization credential handling;
- production release-account, app-store/account operations requiring external account authority;
- secret/credential creation, entry, rotation or disclosure outside an already-authorized safe mechanism;
- paid or financially binding irreversible external-service commitments;
- legal/account-level terms or agreement acceptance on the user's behalf;
- production publication when it crosses one of those external trust/account/legal boundaries;
- physical or external desktop-session validation that cannot be performed with available tooling.

Unsigned/draft artifacts, local validation, documentation and preparatory code may proceed up to those gates.

## Update rule

Every substantive PR that changes implemented capability, active work, validation status, backlog ordering or the exact next-safe-task pointer must update this file in the same PR. Do not copy ADR rationale or detailed architecture into this file.
