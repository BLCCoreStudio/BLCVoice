# Wayland EIS compatibility policy

The initial Wayland adapter requires the active desktop portal/compositor EIS implementation to expose a resumed `ei_text` device. Keyboard-only EIS implementations without `ei_text` are intentionally treated as unsupported by this adapter version.

This keeps dictation text independent from keyboard layouts and avoids silently changing delivery mechanisms. A future fallback, if added, must be represented as a separate capability with its own reliability and permission semantics rather than hidden inside this adapter.
