from pathlib import Path

path = Path("apps/desktop/ui/app.js")
source = path.read_text()

replacements = [
    (
        "  shortcutLifecycleUnlisten: null,\n};",
        "  shortcutLifecycleUnlisten: null,\n  shortcutSessionActive: false,\n};",
    ),
    (
        "function dictationReady() {\n  return Boolean(invoke && selectedDevice() && installedModelAvailable() && !state.testSessionId);\n}",
        "function dictationReady() {\n  return Boolean(\n    invoke &&\n      selectedDevice() &&\n      installedModelAvailable() &&\n      !state.testSessionId &&\n      !state.shortcutSessionActive\n  );\n}",
    ),
    (
        "function updateDictationControls() {\n  const active = state.dictationSessionId !== null;\n  elements.dictationToggle.disabled = state.dictationBusy || (!active && !dictationReady());\n  elements.dictationToggle.classList.toggle(\"recording\", active);\n  elements.dictationButtonLabel.textContent = active ? \"Stop & type\" : \"Start dictation\";\n  elements.dictationCancel.hidden = !active;\n  elements.dictationCancel.disabled = state.dictationBusy;\n  elements.startTest.disabled = state.testBusy || Boolean(state.testSessionId) || active || !selectedDevice();\n}",
        "function updateDictationControls() {\n  const active = state.dictationSessionId !== null;\n  const shortcutActive = state.shortcutSessionActive;\n  elements.dictationToggle.disabled =\n    shortcutActive || state.dictationBusy || (!active && !dictationReady());\n  elements.dictationToggle.classList.toggle(\"recording\", active || shortcutActive);\n  elements.dictationButtonLabel.textContent = shortcutActive\n    ? \"Shortcut dictation active\"\n    : active\n      ? \"Stop & type\"\n      : \"Start dictation\";\n  elements.dictationCancel.hidden = !active;\n  elements.dictationCancel.disabled = shortcutActive || state.dictationBusy;\n  elements.startTest.disabled =\n    state.testBusy || Boolean(state.testSessionId) || active || shortcutActive || !selectedDevice();\n}",
    ),
    (
        "    case \"starting\":\n      state.dictationBusy = true;\n      state.dictationSessionId = null;",
        "    case \"starting\":\n      state.shortcutSessionActive = true;\n      state.dictationBusy = true;\n      state.dictationSessionId = null;",
    ),
    (
        "    case \"recording\":\n      state.dictationBusy = false;\n      state.dictationSessionId = payload.sessionId ?? null;",
        "    case \"recording\":\n      state.shortcutSessionActive = true;\n      state.dictationBusy = false;\n      state.dictationSessionId = null;",
    ),
    (
        "    case \"finishing\":\n      state.dictationBusy = true;\n      if (payload.sessionId != null) state.dictationSessionId = payload.sessionId;",
        "    case \"finishing\":\n      state.shortcutSessionActive = true;\n      state.dictationBusy = true;\n      state.dictationSessionId = null;",
    ),
    (
        "    case \"completed\":\n      state.dictationBusy = false;\n      state.dictationSessionId = null;",
        "    case \"completed\":\n      state.shortcutSessionActive = false;\n      state.dictationBusy = false;\n      state.dictationSessionId = null;",
    ),
    (
        "    case \"failed\":\n      state.dictationBusy = false;\n      state.dictationSessionId = null;",
        "    case \"failed\":\n      state.shortcutSessionActive = false;\n      state.dictationBusy = false;\n      state.dictationSessionId = null;",
    ),
    (
        "    if (status.dictationSessionId && status.dictationState === \"recording\") {\n      state.dictationSessionId = status.dictationSessionId;\n      setPill(elements.dictationState, \"Recording\", \"recording\");\n      elements.dictationMessage.textContent = \"A dictation session was already active.\";",
        "    if (status.dictationSessionId && status.dictationState === \"recording\") {\n      state.dictationSessionId = null;\n      state.shortcutSessionActive = true;\n      setPill(elements.dictationState, \"Recording\", \"recording\");\n      elements.dictationMessage.textContent =\n        \"A backend-owned dictation session was already active. Use Ctrl + Shift + Space to stop it.\";",
    ),
    (
        "    } else if (status.dictationState === \"idle\") {\n      setPill(elements.dictationState, \"Ready\", \"idle\");",
        "    } else if (status.dictationState === \"idle\") {\n      state.shortcutSessionActive = false;\n      setPill(elements.dictationState, \"Ready\", \"idle\");",
    ),
]

for old, new in replacements:
    if old not in source:
        raise SystemExit(f"dashboard ownership marker not found: {old[:80]!r}")
    source = source.replace(old, new, 1)

path.write_text(source)
