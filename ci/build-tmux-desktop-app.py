#!/usr/bin/env python3
"""Package the current Mac debug build as a separately launchable tmux desktop."""
from pathlib import Path
import plistlib
import shutil
import stat
import sys

root = Path(__file__).resolve().parent.parent
if sys.platform != 'darwin':
    raise SystemExit('This packager is for macOS. See docs/tmux-desktop.md for Windows.')
app = root / 'target' / 'WezTerm Tmux.app'
macos = app / 'Contents' / 'MacOS'
resources = app / 'Contents' / 'Resources'
for binary in ('wezterm-gui', 'wezterm'):
    if not (root / 'target/debug' / binary).is_file():
        raise SystemExit(f'Build {binary} first: cargo build -p wezterm-gui -p wezterm')
macos.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)
with (root / 'assets/macos/WezTerm.app/Contents/Info.plist').open('rb') as f:
    info = plistlib.load(f)
info.update(CFBundleName='WezTerm Tmux', CFBundleDisplayName='WezTerm Tmux',
            CFBundleIdentifier='com.github.wez.wezterm.tmux-desktop')
with (app / 'Contents/Info.plist').open('wb') as f:
    plistlib.dump(info, f)
shutil.copy2(root / 'assets/macos/WezTerm.app/Contents/Resources/terminal.icns', resources)
shutil.copy2(root / 'assets/tmux-desktop/wezterm.lua', resources / 'tmux-desktop.lua')
shutil.copy2(root / 'target/debug/wezterm-gui', macos / 'wezterm-gui-bin')
shutil.copy2(root / 'target/debug/wezterm', macos / 'wezterm')
for library in (root / 'assets/macos/WezTerm.app').glob('*.dylib'):
    shutil.copy2(library, macos / library.name)
launcher = macos / 'wezterm-gui'
launcher.write_text('''#!/bin/sh
app_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH"
exec "$app_dir/wezterm-gui-bin" --config-file "$app_dir/../Resources/tmux-desktop.lua" start --always-new-process "$@"
''')
launcher.chmod(launcher.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
print(app)
