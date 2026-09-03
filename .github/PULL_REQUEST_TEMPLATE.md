## Summary

<!-- What problem does this change solve? -->

## Research / evidence

<!-- For material platform/runtime/dependency/architecture choices, link the current official/upstream sources, alternatives considered, and relevant benchmark/test evidence. Write "Not material" for routine changes. -->

## Approach

<!-- Explain the implementation/design and why it fits the current architecture and accepted ADRs. -->

## Validation

<!-- List tests, commands, benchmark evidence and real environments used to validate the change. -->

- [ ] Relevant automated tests pass
- [ ] All applicable critical CI jobs pass, not only the currently configured required-check subset
- [ ] Manual/real-platform testing performed where platform behavior changed or support is claimed
- [ ] Benchmark evidence recorded when the change makes or depends on a measurable performance claim
- [ ] Documentation updated when behavior or architecture changed
- [ ] `PROJECT_STATE.md` updated when implemented capability, active work, validation status, backlog ordering or the next-safe-task pointer changed

## Platform impact

<!-- Windows / Linux X11 / KDE Wayland / GNOME Wayland / macOS / none -->

## Security and privacy

<!-- Note new permissions, network access, data retention, secrets, microphone/clipboard/input behavior, or write "None". -->

## Checklist

- [ ] The change is focused and reviewable
- [ ] New support/compatibility claims are backed by testing
- [ ] Failures are surfaced rather than silently treated as success
- [ ] Architecture-significant decisions include a new/superseding ADR and update the canonical architecture summary when needed
- [ ] No mandatory stop gate in `AGENTS.md` is crossed by this PR
