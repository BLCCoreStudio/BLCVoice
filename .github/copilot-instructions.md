# BLCVoice Copilot Instructions

Use [`../AGENTS.md`](../AGENTS.md) as the canonical agent operating contract.

Before making changes, follow its required read order and authority rules. In particular:

- `ARCHITECTURE.md` remains the canonical current architecture description.
- `docs/adr/` remains the canonical material decision history; do not create a parallel decisions system.
- `PROJECT_STATE.md` is the current operational snapshot and next-task pointer.
- GitHub PRs/issues/CI/rulesets are the live work and gate state.
- Research material platform/runtime/dependency choices against current official or upstream sources before implementation.
- Use branch + PR for substantive work, run the applicable validation, update project state, and continue with the next safe task unless an `AGENTS.md` mandatory stop gate is reached.

Do not duplicate the policies from `AGENTS.md` here. If this file and `AGENTS.md` ever differ, `AGENTS.md` controls and this adapter must be corrected.
