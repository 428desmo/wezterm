#!/usr/bin/env python3
"""Package existing macOS debug binaries using the standard app identity/config."""
from pathlib import Path
import shutil
import sys
root = Path(__file__).resolve().parent.parent
if sys.platform != 'darwin':
    raise SystemExit('This packager is for macOS; use the existing Windows build workflow.')
app = root / 'target/WezTerm.app'
macos = app / 'Contents/MacOS'
resources = app / 'Contents/Resources'
for binary in ('wezterm-gui', 'wezterm'):
    if not (root / 'target/debug' / binary).is_file():
        raise SystemExit(f'Build first: cargo build -p wezterm-gui -p wezterm')
macos.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)
shutil.copy2(root / 'assets/macos/WezTerm.app/Contents/Info.plist', app / 'Contents/Info.plist')
shutil.copy2(root / 'assets/macos/WezTerm.app/Contents/Resources/terminal.icns', resources)
for binary in ('wezterm-gui', 'wezterm'):
    shutil.copy2(root / 'target/debug' / binary, macos / binary)
for library in (root / 'assets/macos/WezTerm.app').glob('*.dylib'):
    shutil.copy2(library, macos / library.name)
print(app)
