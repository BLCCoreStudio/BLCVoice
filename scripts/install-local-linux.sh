#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "BLCVoice local installer only supports Linux." >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
if [[ -z "$repo_root" ]]; then
  echo "Run this script from inside the BLCVoice repository." >&2
  exit 1
fi

cd "$repo_root"

cargo build --release -p blcvoice-desktop

binary_src="$repo_root/target/release/blcvoice"
binary_dst="$HOME/.local/bin/blcvoice"
applications_dir="$HOME/.local/share/applications"
icons_root="$HOME/.local/share/icons/hicolor"
icon_src="$repo_root/apps/desktop/src-tauri/icons"
desktop_file="$applications_dir/blcvoice.desktop"

install -Dm755 "$binary_src" "$binary_dst"
install -Dm644 "$icon_src/32x32.png" "$icons_root/32x32/apps/blcvoice.png"
install -Dm644 "$icon_src/128x128.png" "$icons_root/128x128/apps/blcvoice.png"
install -Dm644 "$icon_src/128x128@2x.png" "$icons_root/256x256/apps/blcvoice.png"

mkdir -p "$applications_dir"
cat > "$desktop_file" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=BLCVoice
GenericName=Voice Dictation
Comment=Private local voice dictation that works where you type
Exec=$binary_dst
Icon=blcvoice
Terminal=false
Categories=AudioVideo;Audio;Utility;
Keywords=voice;dictation;speech;transcription;whisper;
StartupNotify=true
StartupWMClass=blcvoice
DBusActivatable=false
EOF
chmod 0644 "$desktop_file"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$applications_dir" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
  kbuildsycoca6 >/dev/null 2>&1 || true
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
  kbuildsycoca5 >/dev/null 2>&1 || true
fi

echo "BLCVoice installed for this user."
echo "Launcher: $desktop_file"
echo "Binary:   $binary_dst"
echo "Open BLCVoice from your application launcher instead of Konsole to validate desktop identity."
