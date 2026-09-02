# ADR 0017: Desktop shortcut capability reporting

## Status

Accepted.

## Context

The runtime-independent shortcut layer can now distinguish Windows, macOS, Linux/X11 and Linux/Wayland and select the permitted backend class. The desktop host still needs a typed way to expose that decision to diagnostics and later settings UI.

Backend selection and backend registration are different facts. Reporting a selected backend as though the operating system has accepted a shortcut would violate BLCVoice's rule that attempted work is not equivalent to delivered work.

## Decision

The desktop host exposes a `shortcut_capability` command backed by `blcvoice-shortcuts`.

The response includes:

- desktop platform;
- Linux display server when applicable;
- selected shortcut backend when capability resolution succeeds;
- a typed capability error when resolution fails;
- an explicit `registrationImplemented` flag.

Until the concrete registration adapters are wired, `registrationImplemented` remains false. The command therefore describes the backend BLCVoice should use without implying that `Ctrl+Shift+Space` has been registered or accepted by the compositor/operating system.

The desktop layer must consume the shared capability resolver rather than reproduce Linux environment heuristics.

## Consequences

- Diagnostics can explain why a machine needs native/X11 or XDG Portal shortcut handling.
- Wayland cannot be mislabeled as successfully registered before portal binding completes.
- The next shortcut implementation can extend this DTO with actual registration state and typed registration failures without changing capability semantics.
