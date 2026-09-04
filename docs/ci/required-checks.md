# Required CI policy

BLCVoice treats the full cross-platform correctness matrix in `.github/workflows/ci.yml` as merge-blocking.

## Canonical required check

`Required CI` is the stable ruleset-facing status check. It depends on every critical job and runs with `if: always()` so a failed or skipped prerequisite cannot silently turn the gate into a skipped success path.

Critical prerequisites:

- `Cargo lockfile`
- `Core (ubuntu-latest)`
- `Core (windows-latest)`
- `Core (macos-latest)`
- `Rust quality`
- `X11 insertion smoke`
- `Audio backend (ubuntu-24.04)`
- `Audio backend (windows-latest)`
- `Audio backend (macos-latest)`
- `ASR adapter (ubuntu-24.04)`
- `ASR adapter (windows-latest)`
- `ASR adapter (macos-latest)`
- `Desktop shell (Linux)`
- `Desktop shell (windows-latest)`
- `Desktop shell (macos-latest)`

There are currently no intentionally informational jobs inside the `CI` workflow. Packaging, scheduled security audit, dependency automation, release publication, signing/notarization and real-device/session validation are separate concerns and are not represented by `Required CI`.

## Ruleset migration

The repository ruleset must add `Required CI` as a required status check only after that exact check name has successfully reported on the repository. Existing required checks must remain in place during that migration. After the new aggregate gate has been observed and enforced successfully, redundant individual required checks may be removed only in a separate, reviewed ruleset change with evidence that `Required CI` covers the full matrix.

The GitHub connection used by autonomous development does not have repository-administration permission to mutate rulesets. Therefore the workflow and policy can be prepared autonomously, but the ruleset mutation remains an external repository-control action.

## Change discipline

Any new correctness-critical CI job must be added to `required-ci.needs` in the same pull request. Renaming `Required CI` is a breaking governance change because GitHub rulesets reference the displayed job name. Changes to the critical/non-critical classification must update this document and the relevant issue or ADR rationale.
