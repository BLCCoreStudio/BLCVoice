# blcvoice-insertion-x11

X11 text-insertion adapter for BLCVoice.

The adapter uses the XTEST extension for synthetic key events. To remain independent of the user's active keyboard layout, it reserves an otherwise unused, non-modifier X11 keycode, temporarily maps that keycode to each required Unicode keysym, emits KeyPress/KeyRelease through XTEST, and restores the original mapping before returning.

It does not use the clipboard as a hidden fallback and does not claim that successful protocol submission proves the focused application's document changed.
