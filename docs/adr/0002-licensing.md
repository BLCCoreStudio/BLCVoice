# ADR 0002: Dual-license source code under MIT OR Apache-2.0

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice is intended to be an open-source Rust-based desktop application and may later expose reusable crates, SDKs or integration components. The project should remain easy to adopt while providing an explicit patent license option commonly used in the Rust ecosystem.

## Decision

Unless a file or directory explicitly states otherwise, BLCVoice source code is offered under either:

- the MIT License, or
- the Apache License, Version 2.0,

at the recipient's option.

Repository license files are `LICENSE-MIT` and `LICENSE-APACHE`.

Contributions intentionally submitted for inclusion are accepted on the same basis unless explicitly agreed otherwise.

## Trademark and branding

The source-code license does not grant trademark rights. Product names, logos and brand assets may receive separate usage guidelines before formal branding assets are published.

## Third-party components

Dependencies, models and bundled third-party assets retain their own licenses. Distribution work must track those obligations independently of BLCVoice's project license.

## Consequences

- Broad permissive reuse remains possible.
- Apache-2.0 provides an explicit patent grant and termination terms.
- Dependency and model licensing still requires separate review before redistribution.
