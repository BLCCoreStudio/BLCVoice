# ADR 0004: Audio discovery backend

- Status: Accepted
- Date: 2026-09-02

## Context

BLCVoice needs reliable microphone discovery before audio capture, VAD or speech recognition can be implemented. A single generic `default audio device` assumption is not sufficient on Linux, where ALSA, PipeWire and PulseAudio may coexist and expose different behavior.

The project also needs stable device identifiers, structured failure categories and a way to replace the low-level audio runtime later without changing the application core.

## Decision

1. `blcvoice-audio` owns runtime-independent audio discovery types and the `InputDeviceDiscovery` contract.
2. `blcvoice-audio-cpal` is the first low-level adapter and targets CPAL 0.18.x.
3. On Linux, BLCVoice compiles CPAL's native PipeWire and PulseAudio hosts in addition to ALSA. Discovery preference is PipeWire, then PulseAudio, then ALSA, with fallback when a preferred host cannot expose usable input devices.
4. Windows uses the native WASAPI path supplied by CPAL. macOS uses CoreAudio.
5. Persisted microphone selections use CPAL's backend-qualified stable device ID through the adapter; the rest of BLCVoice stores only the runtime-independent `AudioDeviceId` wrapper.
6. Device-native sample rate, channel count and sample format are discovered but not converted by the discovery adapter. Resampling, channel mixing and ASR-specific normalization will be separate pipeline stages.
7. One backend failure must not erase successful discovery from a fallback backend. Failures remain structured and visible for diagnostics.
8. This decision covers device discovery only. It does not yet select a buffering strategy, real-time scheduling policy, capture callback design or VAD implementation.

## Why CPAL first

CPAL provides native WASAPI, CoreAudio and ALSA backends, optional PipeWire/PulseAudio support, stable device IDs, structured device descriptions and a unified non-exhaustive error taxonomy. It remains behind an adapter so BLCVoice can replace or augment it if platform-specific reliability requires native code later.

## Consequences

- Linux builds require ALSA development headers and, for the selected features, PipeWire and PulseAudio development headers.
- Audio backend compilation must be checked on Linux, Windows and macOS in CI.
- Runtime hardware behavior still requires real-device testing; a successful CI compile is not evidence that every microphone/compositor/distribution works.
- Future capture code must preserve typed device/permission/backend failures rather than collapsing them into generic strings.
