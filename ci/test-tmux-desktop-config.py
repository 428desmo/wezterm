#!/usr/bin/env python3
"""Exercise connection-list UI callbacks with mocks, without SSH or GUI input."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='wezterm-connections-test-') as temp:
    temp = Path(temp)
    config = temp / 'test.lua'
    profiles = temp / 'profiles.json'
    lua = r'''
local real = require 'wezterm'
local events, current, spawned, notifications = {}, nil, nil, {}
local action = setmetatable({}, {__index=function(_,name)
 return function(args) return {kind=name,args=args} end
end})
local mock = {home_dir=real.home_dir,json_parse=real.json_parse,json_encode=real.json_encode,
 action=action,action_callback=function(fn)return fn end,config_builder=function()return {} end,
 on=function(name,fn)events[name]=fn end,
 log_error=function(text)table.insert(notifications,text)end,
 mux={spawn_window=function(args)spawned=args;return nil,nil,{set_title=function()end}end}}
local win = {perform_action=function(_,a)current=a end,
 toast_notification=function(_,_,text)table.insert(notifications,text)end,
 mux_window=function()return {get_workspace=function()return 'default'end,active_tab=function()return nil end}end}
package.loaded.wezterm=mock
assert(dofile(PROFILE))
local palette=events['augment-command-palette']()
palette[1].action(win,{})
assert(current.kind=='InputSelector' and current.args.title=='保存済みtmux接続先')
assert(events['new-tab-button-click'](win,{},'Left')==false)
assert(current.args.title=='tmux Desktop')
local function choose(value)
 assert(current.kind=='InputSelector',current.kind)
 current.args.action(win,{},value)
end
local function answer(value)
 assert(current.kind=='PromptInputLine',current.kind)
 current.args.action(win,{},value)
end
local function add(name,target,session,socket)
 events['tmux-desktop-menu'](win,{})
 choose('connect');choose('add');answer(name);answer(target);answer(session);answer(socket)
end
add('Test', 'alice@host', 'smoke', 'wezterm-ext-test')
local f=assert(io.open(STORE));local data=real.json_parse(f:read('*a'));f:close()
assert(data.version==1 and #data.connections==1)
assert(data.connections[1].target=='alice@host')
choose('1')
assert(spawned.args[1]=='ssh' and spawned.args[2]=='-tt' and spawned.args[3]=='alice@host')
assert(spawned.args[4]=="tmux -L 'wezterm-ext-test' -CC attach-session -t '=smoke'")
assert(spawned.workspace=='default')
local original_rename=os.rename
os.rename=function(source,destination)
 if source==STORE..'.tmp' and destination==STORE then
  local existing=io.open(STORE,'r')
  if existing then existing:close();return nil,'simulated Windows existing destination' end
 end
 return original_rename(source,destination)
end
add('Test','alias','session with spaces',"socket'quoted")
os.rename=original_rename
f=assert(io.open(STORE));data=real.json_parse(f:read('*a'));f:close()
assert(#data.connections==1 and data.connections[1].target=='alias')
choose('1')
assert(spawned.args[4]:find("'=session with spaces'",1,true))
assert(spawned.args[4]:find("'socket'\\''quoted'",1,true))
events['tmux-desktop-menu'](win,{});choose('connect');choose('add');answer(nil)
assert(#notifications==0)
local before=spawned
events['tmux-desktop-menu'](win,{});choose('connect');choose('add')
answer('Invalid');answer('-oProxyCommand=anything')
assert(spawned==before and #notifications>0)
events['tmux-desktop-menu'](win,{});choose('delete');choose('1')
f=assert(io.open(STORE));data=real.json_parse(f:read('*a'));f:close()
assert(#data.connections==0)
local moved=nil
local function remote_tab(id,title,window_id)
 return {tab_id=function()return id end,get_title=function()return title end,
  panes=function()return {{get_domain_name=function()return 'tmux'end}}end,
  window=function()return {window_id=function()return window_id end}end,
  move_to_window=function(_,target)moved=target end}
end
local source=remote_tab(7,'source',10)
local dest=remote_tab(8,'dest',20)
local current_window={active_tab=function()return source end,tabs=function()return {source}end,window_id=function()return 10 end}
local dest_window={tabs=function()return {dest}end,window_id=function()return 20 end}
local control={tabs=function()return {{panes=function()return {{get_domain_name=function()return 'local'end}}end}}end,window_id=function()return 30 end}
win.mux_window=function()return current_window end
mock.mux.all_windows=function()return {current_window,dest_window,control}end
mock.mux.get_tab=function(id)assert(id==7);return source end
mock.mux.get_window=function(id)assert(id==20);return dest_window end
events['tmux-desktop-menu'](win,{})
local merge_count=0
for _,entry in ipairs(current.args.choices) do
 if entry.id:match('^merge:') then merge_count=merge_count+1;assert(entry.id=='merge:7:20')end
end
assert(merge_count==1)
choose('merge:7:20');assert(moved==20)
package.loaded.wezterm=real
local out=assert(io.open(RESULT,'w'));out:write('PASS');out:close()
return {}
'''
    lua = lua.replace('PROFILE', json.dumps(str(root/'assets/tmux-desktop/wezterm.lua')))
    lua = lua.replace('STORE', json.dumps(str(profiles))).replace('RESULT', json.dumps(str(temp/'result')))
    config.write_text(lua)
    binary = root/'target/debug/wezterm-gui'
    result = subprocess.run([str(binary),'--config-file',str(config),'show-keys','--lua'],
        env=dict(os.environ,WEZTERM_TMUX_CONNECTIONS_FILE=str(profiles)),capture_output=True,text=True)
    assert result.returncode==0 and (temp/'result').read_text()=='PASS',result.stderr
    print('PASS: register, replace, select, quote, cancel, reject unsafe target, delete; no SSH connection')
