from pathlib import Path


def replace_once(path, old, new):
    file = Path(path)
    source = file.read_text()
    if old not in source:
        raise SystemExit(f"marker not found in {path}: {old!r}")
    file.write_text(source.replace(old, new, 1))


# The desktop host directly names dictation-layer report/error types in this integration.
replace_once(
    "apps/desktop/src-tauri/Cargo.toml",
    'blcvoice-vad = { path = "../../../crates/blcvoice-vad" }\n',
    'blcvoice-dictation = { path = "../../../crates/blcvoice-dictation" }\nblcvoice-vad = { path = "../../../crates/blcvoice-vad" }\n',
)

# VadAnalysis is only needed by deterministic test detectors.
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    'use blcvoice_vad::{VadAnalysis, VadConfig, VoiceActivityDetector};\n',
    'use blcvoice_vad::{VadConfig, VoiceActivityDetector};\n',
)
replace_once(
    "apps/desktop/src-tauri/src/dictation.rs",
    '    use super::*;\n',
    '    use super::*;\n    use blcvoice_vad::VadAnalysis;\n',
)

# Prefer an exhaustive match over a nested let-else for the two finish outcomes.
ipc = Path("apps/desktop/src-tauri/src/ipc.rs")
source = ipc.read_text()
old = '''        let outcome = self
            .dictation
            .finish(session_id)
            .map_err(CommandErrorDto::from)?;
        let DesktopDictationFinish::Transcribed(report) = outcome else {
            let DesktopDictationFinish::NoSpeech(report) = outcome else {
                unreachable!();
            };
            return Ok(DictationReportDto::no_speech(report));
        };
'''
new = '''        let report = match self
            .dictation
            .finish(session_id)
            .map_err(CommandErrorDto::from)?
        {
            DesktopDictationFinish::NoSpeech(report) => {
                return Ok(DictationReportDto::no_speech(report));
            }
            DesktopDictationFinish::Transcribed(report) => report,
        };
'''
if old in source:
    ipc.write_text(source.replace(old, new, 1))
