# Wayland insertion security boundary

BLCVoice's Wayland insertion path does not access `/dev/input`, `/dev/uinput`, evdev devices or privileged helpers. It relies on the desktop portal and compositor-provided EIS connection.

The portal request is limited to keyboard control. Pointer and touchscreen capabilities are outside the dictation requirement and are not requested.

A restore token is a permission-related configuration value and should be stored as application configuration rather than logged or exposed in diagnostics. A future persistence layer must support token replacement because portal restore tokens may rotate.
