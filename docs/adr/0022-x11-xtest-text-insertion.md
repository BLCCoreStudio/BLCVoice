# ADR 0022: X11 text insertion uses XTEST with a temporary Unicode mapping

## Status

Accepted.

## Context

XTEST emits key events by keycode rather than arbitrary Unicode text. Using only the active keyboard layout would make multilingual dictation unreliable.

## Decision

The X11 adapter requires XTEST and selects a keycode whose current mapping is empty and which is not used as a modifier. It snapshots that mapping, temporarily assigns the required keysym, emits a press/release pair, and restores the original mapping before returning.

Printable Latin-1 characters use their direct X11 keysym values. Other Unicode scalar values use the standard Unicode keysym encoding. Newline and tab use their conventional X11 keysyms. Unsupported control characters are rejected.

Successful submission means X11 accepted the complete sequence. It does not prove that the focused application's document changed. Failures after some text has been submitted are reported as partial submission.

## Consequences

The result is independent of the active keyboard layout and does not require clipboard mutation. The server keyboard map is temporarily changed on one previously unused keycode, so runtime validation must verify restoration and behavior across common X11 desktops before production wiring.
