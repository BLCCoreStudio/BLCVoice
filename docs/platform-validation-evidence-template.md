# Platform validation evidence record

Use one copy of this template per environment and tested BLCVoice commit. Do not mark a row `VALIDATED` without attaching or linking the completed record.

## Environment

- BLCVoice commit:
- Build/package provenance:
- OS + version:
- Architecture:
- Desktop environment:
- Session/compositor:
- Portal frontend/backend versions (Wayland):
- Active insertion backend:
- ASR engine/model/backend:
- Microphone device:
- Initial permission state:
- Test date/time + timezone:

## Results

| Scenario | Result | Evidence / notes |
| --- | --- | --- |
| Shortcut -> capture starts | NOT_RUN | |
| Microphone denial path | NOT_RUN | |
| VAD/endpointer stop path | NOT_RUN | |
| Local ASR transcript produced | NOT_RUN | |
| Insertion capability resolved truthfully | NOT_RUN | |
| Turkish UTF-8 text delivered | NOT_RUN | |
| Emoji delivered | NOT_RUN | |
| Multiline delivery where supported | NOT_RUN | |
| Long-text delivery | NOT_RUN | |
| Target focus preserved | NOT_RUN | |
| Insertion permission denial/revocation | NOT_RUN | |
| Backend/protocol accepted submission | NOT_RUN | |
| Target document semantically verified | NOT_RUN | |
| Restart/shutdown cleanup | NOT_RUN | |
| No forbidden fallback | NOT_RUN | |

Allowed result values: `PASS`, `FAIL`, `UNSUPPORTED`, `NOT_RUN`, `BLOCKED_EXTERNAL`.

## Wayland-only evidence

- `XDG_SESSION_TYPE`:
- Desktop/compositor version:
- `xdg-desktop-portal` version:
- portal backend package/version:
- RemoteDesktop interface/version observed:
- keyboard-only device request confirmed:
- portal prompt shown:
- denial/cancel result:
- `Start()` result:
- `ConnectToEIS()` result:
- EIS capability/device state:
- restore token behavior (if enabled):
- revocation behavior:
- confirmed no X11/XWayland/evdev/uinput/root fallback:

## Windows-only evidence

- Target process integrity relation to BLCVoice:
- same-integrity Unicode delivery:
- higher-integrity/UIPI-blocked case result:
- returned insertion outcome matches observed delivery:

## macOS-only evidence

- relevant native permission state:
- denial behavior:
- revocation behavior:
- Unicode delivery to representative native text field:
- returned insertion outcome matches observed delivery:

## Final classification

- Matrix status: `UNVALIDATED` / `PARTIAL` / `VALIDATED` / `UNSUPPORTED` / `BLOCKED_EXTERNAL`
- Backend/protocol acceptance proven: yes/no
- Semantic target-document verification proven: yes/no
- Remaining limitations:
- Evidence files/links:
