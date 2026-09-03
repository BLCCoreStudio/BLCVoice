# BLCVoice Agent Operating Contract

This file is the canonical operating contract for autonomous development in BLCVoice. It does not replace the product architecture or decision history.

## Canonical sources of truth

Use the repository sources below for distinct purposes. Do not create parallel documents that duplicate them.

1. `docs/adr/` — canonical record of accepted material product, architecture, platform, security and runtime decisions. A newer accepted ADR may explicitly supersede an older ADR.
2. `ARCHITECTURE.md` — canonical consolidated description of the architecture that is intended to be true now.
3. `SECURITY.md` — security, privacy and disclosure boundaries.
4. `CONTRIBUTING.md` — contribution, PR, compatibility-claim and review policy.
5. `PROJECT_STATE.md` — current implementation snapshot, active work, known validation gaps and the current next-safe-task pointer. It is operational state, not an architecture or decision record.
6. GitHub pull requests, issues, rulesets and CI — live work state and executable repository gates. Live GitHub state overrides stale operational status in `PROJECT_STATE.md`; reconcile the file in the next relevant change.
7. Code, tests, manifests and configuration — executable implementation truth. Implementation drift does not silently supersede an accepted ADR or architecture rule; resolve the conflict explicitly.
8. `README.md` — public product overview. It must not become a second project-state or decision system.

Do not add `DECISIONS.md`. Do not add a separate roadmap or agent-instruction system unless the existing canonical mechanism cannot represent the needed information without ambiguity.

## Required read order for every autonomous run

Before selecting or implementing work:

1. Confirm repository identity, default branch, current `main` HEAD, branch/ruleset protection, latest CI state, and currently open pull requests and issues.
2. Read this `AGENTS.md` completely.
3. Read `PROJECT_STATE.md` and compare it with live GitHub state. Treat differences as state drift to be corrected, not as permission to ignore live state.
4. Read `ARCHITECTURE.md`.
5. Enumerate `docs/adr/`; read foundational ADRs and every ADR relevant to the task. Follow superseding ADR chains when present.
6. Read `SECURITY.md` and `CONTRIBUTING.md`.
7. Read the relevant code, tests, manifests, CI configuration, platform-validation documents and dependency configuration for the task.
8. Re-check the active PR/issue and current base HEAD immediately before creating a branch or changing an existing branch so overlapping work is not duplicated.

`README.md` is useful for product-facing context but is not authoritative for fast-moving implementation state.

## Autonomous development loop

For each substantive task, use this loop:

`RESEARCH -> COMPARE -> DECIDE -> ADR IF MATERIAL -> IMPLEMENT -> TEST -> CROSS-PLATFORM VALIDATE -> BENCHMARK WHEN RELEVANT -> REVIEW FAILURE MODES -> UPDATE PROJECT STATE -> SELECT NEXT SAFE TASK`

A successful compile is not the definition of completion. Completion requires the evidence appropriate to the claim being made.

## Research-first engineering

Before a material implementation, dependency, platform, runtime or architecture choice:

- inspect current official documentation, specifications and upstream repositories first;
- inspect current upstream release notes, issues or source when documentation is insufficient or behavior is version-sensitive;
- compare realistic alternatives on correctness, maintainability, performance, security, permission surface, cross-platform behavior and long-term support;
- prefer production-grade upstream mechanisms over hacks, privilege bypasses or undocumented behavior;
- use reproducible experiments and benchmark results when an engineering claim is measurable;
- record material evidence and trade-offs in the PR and, when applicable, the ADR.

This especially applies to Rust/Tauri architecture, native Windows/macOS/Linux APIs, Wayland/X11 behavior, global shortcuts, microphone/audio capture, realtime audio, VAD, ASR/model runtimes, acceleration, text insertion, accessibility/permissions, local storage/privacy, latency/resource use, packaging/distribution and dependency selection.

Do not manufacture a comparison when only one technically valid option exists, but do not select the first working option when meaningful alternatives exist.

## ADR policy

Create a new ADR, or a superseding ADR, when a decision materially changes one or more of the following:

- product-scope boundary or a foundational product principle;
- crate/module ownership or architecture layering;
- lifecycle, concurrency, cancellation or stale-work semantics;
- ASR/VAD engine contracts or production runtime policy;
- platform capability resolution, native API strategy, global-shortcut strategy or text-insertion strategy;
- fallback or privilege/permission strategy;
- persistence, local-history schema, migration or data-retention policy;
- telemetry, network behavior, model trust/download/verification policy or secrets boundary;
- hardware acceleration policy or packaging/runtime support policy;
- compatibility/support claims or release/signing trust boundaries;
- an existing accepted ADR.

Routine bug fixes, tests, implementation-local refactors, UI polish and changes that remain inside an accepted contract normally do not need an ADR.

When evidence justifies changing an accepted decision, do not rewrite history. Add a superseding ADR that states the evidence, alternatives, rationale and trade-offs, then update `ARCHITECTURE.md` and other affected canonical summaries in the same change.

## Autonomous authority

The agent may proceed without asking for routine engineering approval when the decision can be resolved through documentation, source inspection, testing, benchmarking or engineering judgment.

The agent may autonomously:

- create focused branches, commits, issues and pull requests;
- choose implementation details and dependencies consistent with accepted architecture and policy;
- refactor internal code while preserving contracts;
- add or improve tests, CI, benchmarks, diagnostics and documentation;
- fix regressions and cross-platform defects;
- add or supersede ADRs when the evidence standard above is met;
- update `ARCHITECTURE.md` to match an accepted superseding decision;
- rerun and repair CI failures;
- merge a safe PR when all applicable repository, review, validation and evidence gates pass and no mandatory stop gate is crossed, if repository permissions and policy permit it;
- immediately continue with the next safe task after a merge instead of waiting for a conversational "continue" instruction.

Never treat lack of a user reply as approval for a mandatory stop-gate action.

## Mandatory stop gates

Stop and require explicit user action or approval before performing an action that requires or commits any of the following:

- production code-signing, notarization or signing credentials;
- production release-account, app-store, certificate-authority or similar account-level actions;
- creating, entering, rotating, exposing or otherwise handling user secrets or credentials that are not already safely available through an authorized mechanism;
- starting a paid or financially binding external service, especially an irreversible commitment;
- accepting legal terms, licenses, contracts or account-level agreements on the user's behalf;
- publishing a production release/store submission when that act itself carries an external trust/account/legal gate.

It is acceptable to prepare unsigned bundles, draft release artifacts, configuration, documentation and exact instructions up to the boundary.

If a genuinely unresolved product choice cannot be determined from evidence without changing the product thesis, record the ambiguity and stop rather than guessing. This exception is not a reason to escalate ordinary technical decisions.

## Validation and completion gates

A task is not complete merely because GitHub currently allows merge.

Before completion:

- run all tests relevant to the changed surface;
- run formatting/lint/static checks required by the repository;
- require all critical CI jobs relevant to the change to pass, including critical jobs that may not yet be configured as branch-protection required checks;
- consider Windows, macOS, Linux/X11 and Linux/Wayland effects for desktop/platform changes;
- perform real-platform validation when behavior depends on a real compositor, permission broker, device, target application or OS API that CI cannot faithfully emulate;
- mark an environment experimental/unvalidated rather than claiming support without evidence;
- benchmark latency, memory, resource use, accuracy or throughput when the PR makes or depends on a measurable performance claim;
- review denial, cancellation, unavailable-backend, device-loss, stale-session and partial-delivery failure modes relevant to the change;
- update documentation and `PROJECT_STATE.md` in the same PR when current state or next-task ordering changes.

A protocol/backend receipt must not be described as semantic document mutation unless that stronger property was actually verified.

## Task selection and continuation

Use persistent GitHub work state instead of a duplicate `ROADMAP.md`.

Select the next safe task in this order:

1. security regressions, data-loss risks, broken `main` or critical CI failures;
2. an already-active, non-obsolete PR that is closest to completion and does not cross a stop gate;
3. the explicit `Next safe task` in `PROJECT_STATE.md`, if it still matches live state;
4. open GitHub issues, prioritizing core dictation reliability, correctness, safety and release-blocking infrastructure before broader features;
5. if the queue is empty, derive the next smallest task from accepted product scope, architecture gaps, validation gaps or benchmark evidence, verify that no duplicate work exists, create a GitHub issue, then execute it.

Keep one logical task per PR where practical. After a safe PR is merged, reconcile `PROJECT_STATE.md`, select the next safe task and continue automatically.

Do not allow broader AI integrations, meeting features or adjacent feature expansion to displace the reliability of the core dictation loop defined by the accepted product-scope ADR.

## Project-state discipline

`PROJECT_STATE.md` must remain compact and operational. It may contain:

- last reconciliation date/baseline;
- current development stage;
- completed capabilities at a high level;
- active PRs;
- open canonical backlog issues;
- known validation/CI gaps;
- exact next safe task;
- mandatory external gates.

It must not duplicate ADR rationale, detailed architecture, long-term product strategy or a full roadmap.
