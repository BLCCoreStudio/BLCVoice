# BLCVoice application icon

`app-icon.svg` is the canonical BLCVoice application icon source.

The generated platform assets are committed so Tauri bundle builds are deterministic and do not require icon-generation tooling:

- `32x32.png` — Linux small icon
- `128x128.png` — Linux standard icon
- `128x128@2x.png` — Linux HiDPI icon
- `icon.icns` — macOS
- `icon.ico` — Windows
- `icon.png` — high-resolution source/fallback asset used by local tooling

The mark intentionally reuses BLCVoice's waveform visual language: a dark rounded-square field, violet voice bars, and a small local/private-state accent. Changes to the visual identity should start from `app-icon.svg` and regenerate the platform set rather than editing generated binaries independently.
