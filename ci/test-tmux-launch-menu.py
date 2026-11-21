#!/usr/bin/env python3
"""Check standard launcher config and built-in rename actions without SSH."""
import json
from pathlib import Path
import subprocess
import tempfile
root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='wezterm-launch-menu-') as temp:
    config = Path(temp) / 'test.lua'
    lua = r'''local w = require 'wezterm'
w.on = function(name) error('Example must not replace GUI behavior: '..name) end
local config = dofile(EXAMPLE)
assert(config.tmux_window_per_os_window)
assert(#config.launch_menu == 2)
for _,entry in ipairs(config.launch_menu) do
 assert(entry.domain.DomainName == 'local', 'Launcher commands must run locally')
end
config.keys = {
 {key='r',mods='CTRL|ALT',action=w.action.RenameTab},
 {key='w',mods='CTRL|ALT',action=w.action.RenameWindow},
}
return config
'''
    config.write_text(lua.replace('EXAMPLE', json.dumps(str(root / 'assets/tmux-launch-menu.lua'))))
    result = subprocess.run([str(root / 'target/debug/wezterm-gui'), '--config-file', str(config), 'show-keys', '--lua'], capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'RenameTab' in result.stdout and 'RenameWindow' in result.stdout, result.stdout
    print('PASS: standard launch_menu, built-in rename actions, no private startup or palette hook; no SSH connection')
