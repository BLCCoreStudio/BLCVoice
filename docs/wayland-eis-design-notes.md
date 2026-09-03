# Wayland EIS design notes

The implementation follows the same protocol-level constraints as libei itself rather than assuming that a generic synthetic-keyboard API is available on Wayland.

Key invariants:

- portal consent is compositor mediated;
- only keyboard control is requested;
- `ei_text` is required for the initial adapter;
- UTF-8 payloads are bounded to the protocol's per-request limit;
- EIS events are continuously drained by a dedicated owner thread;
- transport submission and semantic text delivery remain separate concepts.

These notes are intentionally implementation-focused. ADR 0021 is the normative architecture decision.
