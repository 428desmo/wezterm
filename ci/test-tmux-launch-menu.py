#!/usr/bin/env python3
"""Load the example with real Lua actions; verify palette callbacks without SSH."""
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='wezterm-launch-menu-') as temp:
    config = Path(temp) / 'test.lua'
    lua = r'''
local w = require 'wezterm'
local palette, callbacks = nil, {}
local original_callback = w.action_callback
w.on = function(name, fn)
 assert(name == 'augment-command-palette', 'Unexpected private/startup hook: '..name)
 palette = fn
end
w.action_callback = function(fn)
 table.insert(callbacks, fn)
 return original_callback(fn)
end
local config = dofile(EXAMPLE)
assert(config.tmux_window_per_os_window)
assert(#config.launch_menu == 2)
for _,entry in ipairs(config.launch_menu) do
 assert(entry.domain.DomainName == 'local', 'Launcher commands must run locally even from a tmux pane')
end
local renamed, kind, selected = {}, 'tmux', nil
local tab = {
 get_title=function()return 'Old title'end,
 active_pane=function()return {get_domain_name=function()return kind end}end,
 rename_tmux_window=function(_,line)renamed.server=line end,
 set_title=function(_,line)renamed.tab=line end,
}
local mw = {get_title=function()return 'Old window'end,set_title=function(_,line)renamed.window=line end}
selected=tab
local win={active_tab=function()return selected end,mux_window=function()return mw end}
local entries=palette(win,{})
assert(#entries==2 and entries[2].brief=='Rename tmux window')
-- Cancelling must not rename; changing the active tab while the prompt is open
-- must not apply the title to the newly selected tab.
callbacks[2](win,{},nil);assert(renamed.server==nil)
selected={};callbacks[2](win,{},"work 'quoted' 日本語")
assert(renamed.server=="work 'quoted' 日本語" and renamed.tab==nil)
callbacks[1](win,{},'');assert(renamed.window=='')
callbacks={};selected=tab;kind='local'
entries=palette(win,{})
assert(entries[2].brief=='Rename tab')
callbacks[2](win,{},'Local title');assert(renamed.tab=='Local title')
selected=nil;assert(#palette(win,{})==1)
w.log_info('PASS: standard launch_menu, synchronous palette, cancel, stable rename target, tmux/local title behavior')
return config
'''
    config.write_text(lua.replace('EXAMPLE', json.dumps(str(root / 'assets/tmux-launch-menu.lua'))))
    result = subprocess.run([str(root / 'target/debug/wezterm-gui'), '--config-file', str(config), 'show-keys'], capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    assert 'PASS: standard launch_menu' in result.stderr, result.stdout + result.stderr
    print('PASS: standard launch_menu, synchronous palette, cancel, stable rename target, tmux/local titles; no SSH connection')
