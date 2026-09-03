# Wayland EIS runtime validation

The `blcvoice-insertion-eis` crate is compile-, lint- and unit-testable without a graphical session, but its compositor behavior must be exercised on real Wayland desktops before the adapter is wired into production dictation.

Minimum manual validation matrix:

- KDE Plasma 6 on Wayland with `xdg-desktop-portal-kde`
- GNOME on Wayland with its active portal backend

For each environment verify:

1. BLCVoice requests keyboard control without pointer/touchscreen permission.
2. Portal denial or cancellation returns a permission error without crashing the app.
3. A successful session exposes a resumed EIS device with `ei_text` or reports the capability as unavailable.
4. Turkish characters, emoji and text longer than 254 UTF-8 bytes are submitted without splitting a code point.
5. Focus remains on the original target application while text is submitted.
6. Closing/revoking the portal session transitions subsequent insertion attempts to an unavailable/failure state.
7. A returned restore token can be persisted and supplied to a later session, while still respecting the portal's revocation policy.
8. No X11, evdev, uinput or privileged fallback is activated on failure.

Passing this matrix is required before the Wayland adapter is described as end-to-end validated.
