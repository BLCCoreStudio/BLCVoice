from pathlib import Path
import runpy

runpy.run_path("scripts/_integrate_recognizer_cache_base.py", run_name="__main__")

path = Path("apps/desktop/src-tauri/src/dictation.rs")
source = path.read_text()
old = '''        if let Some(cached) = self.lock_recognizer_cache().take() {
            if cached.key == key {
                return Ok((key, cached.recognizer));
            }
        }
'''
new = '''        if let Some(cached) = self.lock_recognizer_cache().take()
            && cached.key == key
        {
            return Ok((key, cached.recognizer));
        }
'''
if old not in source:
    raise SystemExit("recognizer cache block not found")
path.write_text(source.replace(old, new, 1))
