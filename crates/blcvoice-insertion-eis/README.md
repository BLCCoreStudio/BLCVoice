# blcvoice-insertion-eis

Wayland text-insertion adapter for BLCVoice.

This crate opens an XDG RemoteDesktop portal session, requests only keyboard control, connects to the compositor's EIS endpoint and submits UTF-8 text through `ei_text` when available.

A successful insertion receipt represents complete protocol submission, not proof that the focused application's document changed.

The adapter intentionally does not fall back to X11, evdev, uinput or elevated input-device access on Wayland.
