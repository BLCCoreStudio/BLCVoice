# CI required-check policy

BLCVoice treats the `Critical validation gate` job in `.github/workflows/ci.yml` as the canonical merge-blocking summary of the critical correctness matrix.

The gate depends on and requires successful completion of:

- Cargo lockfile
- Core (Ubuntu, Windows, macOS matrix)
- Rust quality
- X11 insertion smoke
- Audio backend (Ubuntu, Windows, macOS matrix)
- ASR adapter (Ubuntu, Windows, macOS matrix)
- Desktop shell (Linux)
- Desktop shell (Windows, macOS matrix)

The gate uses `if: ${{ always() }}` and explicitly fails unless every dependency result is `success`. This is intentional: GitHub can treat a skipped required check as satisfying a required-check rule, so a dependency failure must not turn the aggregate gate into a skipped check.

## Ruleset target

Repository ruleset `main-protection` must retain its existing required checks and add:

- `Critical validation gate` (GitHub Actions)

Once that check has completed successfully in the repository and the ruleset is updated, the aggregate gate becomes the stable future merge-protection contract. Individual existing required checks may be consolidated only in a later, separately reviewed ruleset change with evidence that the aggregate gate is active and fail-closed.

## Informational/non-merge-blocking workflows

Packaging/release workflows are intentionally separate from normal pull-request correctness CI. Production signing, notarization, publication, and real desktop-session compatibility evidence remain external or release-stage gates and must not be represented as compile-time/runtime proof.
